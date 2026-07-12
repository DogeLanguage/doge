use super::*;

impl Codegen {
    pub(super) fn call(
        &self,
        callee: &Expr,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        match callee {
            Expr::Ident { name, .. } => {
                // A local shadows every other meaning — call through its value. A
                // known nested function still gets a compile-time arity check, but
                // it is reached through the dispatcher, so keyword arguments (which
                // need a compile-time-known target) are not available.
                if emit.locals.contains_key(name) {
                    if let Some(params) = emit.local_funcs.get(name) {
                        self.reject_kwargs(
                            kwargs,
                            span,
                            &format!("call {name} without keyword arguments"),
                        )?;
                        self.check_positional_arity(name, params, args.len(), span)?;
                    } else {
                        self.reject_kwargs(
                            kwargs,
                            span,
                            "call this by a known function name to use keyword arguments",
                        )?;
                    }
                    let callee_val = self.resolve_read(emit, name);
                    return self.indirect_call(&callee_val, args, emit);
                }
                if crate::builtins::is_builtin(name) {
                    self.reject_kwargs(
                        kwargs,
                        span,
                        &format!("{name} takes positional arguments only"),
                    )?;
                    return self.builtin_call(name, args, span, emit);
                }
                if let Some(params) = emit.table().funcs.get(name) {
                    let (prelude, parts) =
                        self.resolve_args(name, params, args, kwargs, span, emit)?;
                    let target = func_wrapper(emit.file_id, name);
                    return Ok(self.finish_call(emit, &prelude, parts, &target));
                }
                // `Shibe(...)` — a constructor, resolved statically.
                if emit.class(name).is_some() {
                    return self.constructor_call(name, args, kwargs, span, emit);
                }
                // A module name is not itself callable — you call one of its members.
                if let Some(member) = self.module_first_member(emit, name) {
                    return Err(self
                        .diag(span, format!("{name} is a module, not a function"))
                        .with_headline("very module. much confuse.")
                        .with_hint(format!("call a member — {name}.{member}(…)")));
                }
                // A top-level variable holding a function value: call it indirectly.
                self.reject_kwargs(
                    kwargs,
                    span,
                    "call this by a known function name to use keyword arguments",
                )?;
                let callee_val = format!("env.{}.clone()", field_name(emit.file_id, name));
                self.indirect_call(&callee_val, args, emit)
            }
            // `nerd.sqrt(16)` / `utils.square(6)` — a member call on a module.
            Expr::Attr { obj, name, .. }
                if matches!(obj.as_ref(), Expr::Ident { name: base, .. }
                    if emit.module(base).is_some() || emit.user_module(base).is_some()) =>
            {
                let Expr::Ident { name: base, .. } = obj.as_ref() else {
                    unreachable!("compiler bug: guarded to an Ident base")
                };
                if let Some(module) = emit.module(base) {
                    self.reject_kwargs(
                        kwargs,
                        span,
                        &format!("{base}.{name} takes positional arguments only"),
                    )?;
                    self.module_call(base, module, name, args, span, emit)
                } else {
                    let fid = emit
                        .user_module(base)
                        .expect("compiler bug: module vanished");
                    self.user_module_call(base, fid, name, args, kwargs, span, emit)
                }
            }
            // `kabosu.speak(...)` — a method call, dispatched at runtime, so it takes
            // positional arguments only.
            Expr::Attr { obj, name, .. } => {
                self.reject_kwargs(kwargs, span, "pass these arguments positionally")?;
                emit.uses_method_call.set(true);
                let recv = self.expr(obj, emit)?;
                let mut arg_parts = Vec::with_capacity(args.len());
                for arg in args {
                    arg_parts.push(self.expr(arg, emit)?);
                }
                let call = format!(
                    "call_method({recv}, \"{}\", vec![{}], &mut *env)",
                    escape_str(name),
                    arg_parts.join(", ")
                );
                Ok(self.fail(emit, call))
            }
            // Any other callee expression — `f()()`, `xs[0]()` — is called through
            // the value it evaluates to.
            _ => {
                self.reject_kwargs(
                    kwargs,
                    span,
                    "call this by a known function name to use keyword arguments",
                )?;
                let callee_val = self.expr(callee, emit)?;
                self.indirect_call(&callee_val, args, emit)
            }
        }
    }

    /// Keyword arguments are only accepted where the callee is known at compile
    /// time (a top-level user function, a constructor, or a user-module function).
    /// Everywhere else they are a compile error with a `hint` on what to do.
    pub(super) fn reject_kwargs(
        &self,
        kwargs: &[(String, Expr)],
        span: Span,
        hint: &str,
    ) -> Result<(), Diagnostic> {
        if kwargs.is_empty() {
            return Ok(());
        }
        Err(self
            .diag(
                span,
                "keyword arguments only work when doge knows the function at compile time",
            )
            .with_headline("very keyword. much dynamic.")
            .with_hint(hint.to_string()))
    }

    /// Call a function *value*: type-check the callee, then dispatch through
    /// `call_function`. Both steps are fallible and routed through [`Codegen::fail`].
    pub(super) fn indirect_call(
        &self,
        callee_val: &str,
        args: &[Expr],
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        emit.uses_call_function.set(true);
        let mut arg_parts = Vec::with_capacity(args.len());
        for arg in args {
            arg_parts.push(self.expr(arg, emit)?);
        }
        // `&*` derefs the `Rc<FunctionData>` explicitly: relying on deref coercion
        // through the `?` here trips rustc's expected-type propagation.
        let func = self.fail(emit, format!("callee_function(&{callee_val})"));
        let call = format!(
            "call_function(&*{func}, vec![{}], &mut *env)",
            arg_parts.join(", ")
        );
        Ok(self.fail(emit, call))
    }

    /// A constructor call `Shibe(args)`: resolve against `init`'s parameters (with
    /// defaults, variadic, and keyword arguments), then a `n_<id>(args…, &mut
    /// *env)` through the fail suffix.
    pub(super) fn constructor_call(
        &self,
        name: &str,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        let class = emit
            .class(name)
            .expect("compiler bug: constructor for an unknown class");
        self.emit_constructor_call(name, class, args, kwargs, span, emit)
    }

    /// Emit a constructor call `n_<id>(args…, &mut *env)` for `class`, resolving
    /// `args`/`kwargs` against its `init` header. `label` is the callee name shown
    /// in arity/keyword diagnostics (`Shibe` locally, `utils.Shibe` cross-file).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_constructor_call(
        &self,
        label: &str,
        class: &Class,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        let init_params = init_params(emit.classes, class);
        let (prelude, parts) = self.resolve_args(label, init_params, args, kwargs, span, emit)?;
        let target = format!("{CTOR_PREFIX}{}", class.id);
        Ok(self.finish_call(emit, &prelude, parts, &target))
    }

    /// A stdlib member call `module.member(args)`: static arity against the table,
    /// then `{runtime_fn}(&a0, &a1, …)` through the fail suffix. Calling a const,
    /// or an unknown member, is a real error.
    pub(super) fn module_call(
        &self,
        module_name: &str,
        module: &Module,
        member: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        let func = match module.func(member) {
            Some(func) => func,
            None => {
                if module.const_expr(member).is_some() {
                    return Err(self
                        .diag(
                            span,
                            format!("{module_name}.{member} is a constant, not a function"),
                        )
                        .with_headline("very module. much confuse.")
                        .with_hint(format!("use it as a value — bark {module_name}.{member}")));
                }
                return Err(self.unknown_member(module_name, member, module, span));
            }
        };
        if args.len() != func.arity {
            let noun = if func.arity == 1 {
                "argument"
            } else {
                "arguments"
            };
            return Err(self
                .diag(
                    span,
                    format!(
                        "{module_name}.{member} takes {} {noun}, got {}",
                        func.arity,
                        args.len()
                    ),
                )
                .with_headline(ARITY_HEADLINE)
                .with_hint(func.hint));
        }
        let mut parts = Vec::with_capacity(args.len());
        for arg in args {
            parts.push(format!("&{}", self.expr(arg, emit)?));
        }
        let call = format!("{}({})", func.runtime_fn, parts.join(", "));
        Ok(self.fail(emit, call))
    }

    /// A user module member call `utils.square(args)`: resolve against the
    /// module's function table (with defaults, variadic, and keyword arguments),
    /// then a direct call to its mangled wrapper. Calling a constant, or an unknown
    /// member, is a real error.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn user_module_call(
        &self,
        module_name: &str,
        fid: u32,
        member: &str,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        // `utils.Shibe(…)` — a constructor for one of the module's classes.
        if let Some(class) = emit.class_in(fid, member) {
            let label = format!("{module_name}.{member}");
            return self.emit_constructor_call(&label, class, args, kwargs, span, emit);
        }
        let table = &emit.tables[fid as usize];
        let params = match table.funcs.get(member) {
            Some(params) => params,
            None => {
                if table.consts.contains(member) {
                    return Err(self
                        .diag(
                            span,
                            format!("{module_name}.{member} is a constant, not a function"),
                        )
                        .with_headline("very module. much confuse.")
                        .with_hint(format!("use it as a value — bark {module_name}.{member}")));
                }
                return Err(self.unknown_user_member(emit, module_name, fid, member, span));
            }
        };
        let label = format!("{module_name}.{member}");
        let (prelude, parts) = self.resolve_args(&label, params, args, kwargs, span, emit)?;
        let target = func_wrapper(fid, member);
        Ok(self.finish_call(emit, &prelude, parts, &target))
    }

    /// `super.method(args)` inside a method body: resolve `method` statically to
    /// the nearest ancestor of the enclosing class that defines it, then call that
    /// ancestor's `mf_` wrapper with the current `self` as the receiver. The
    /// checker validates the context; the fallbacks here keep direct codegen (with
    /// no prior check pass) non-panicking.
    pub(super) fn super_call(
        &self,
        method: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        let class = emit
            .current_class
            .and_then(|id| emit.classes.iter().find(|c| c.id == id))
            .ok_or_else(|| {
                self.diag(span, "super only works inside a method")
                    .with_headline("very super. much lost.")
                    .with_hint("use super inside a method of a class with a parent")
            })?;
        let parent = class
            .parent
            .and_then(|pid| emit.classes.iter().find(|c| c.id == pid))
            .ok_or_else(|| {
                self.diag(
                    span,
                    format!("{} has no parent to call super on", class.name),
                )
                .with_headline("very super. much orphan.")
                .with_hint(format!(
                    "give it a parent — many {} much Parent:",
                    class.name
                ))
            })?;
        let (def, params) = effective_methods(emit.classes, parent)
            .into_iter()
            .find(|(name, _, _)| *name == method)
            .map(|(_, def, params)| (def, params))
            .ok_or_else(|| {
                self.diag(
                    span,
                    format!("no parent of {} has a method {method}", class.name),
                )
                .with_headline("very super. much unknown.")
                .with_hint(format!("check the method name — super.{method}(…)"))
            })?;

        let label = format!("{}.{method}", def.name);
        let (prelude, mut parts) = self.resolve_args(&label, params, args, &[], span, emit)?;
        parts.insert(0, self.resolve_read(emit, "self"));
        let target = format!("{METHOD_PREFIX}{}_{method}", def.id);
        Ok(self.finish_call(emit, &prelude, parts, &target))
    }

    pub(super) fn builtin_call(
        &self,
        name: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        let builtin = self.check_builtin_arity(name, args, span)?;
        match builtin.shape {
            BuiltinShape::Fallible => {
                let arg = self.expr(&args[0], emit)?;
                Ok(self.fail(emit, format!("{}(&{arg})", builtin.runtime_fn)))
            }
            BuiltinShape::Infallible => {
                let arg = self.expr(&args[0], emit)?;
                Ok(format!("{}(&{arg})", builtin.runtime_fn))
            }
            BuiltinShape::Range if args.len() == 1 => Ok(self.fail(
                emit,
                format!("range(&Value::Int(0i64), &{})", self.expr(&args[0], emit)?),
            )),
            BuiltinShape::Range => Ok(self.fail(
                emit,
                format!(
                    "range(&{}, &{})",
                    self.expr(&args[0], emit)?,
                    self.expr(&args[1], emit)?
                ),
            )),
        }
    }

    /// Builtin arity is statically known (the accepted counts live in the builtin
    /// table). Returns the resolved builtin so the caller can emit by its shape.
    pub(super) fn check_builtin_arity(
        &self,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<&'static BuiltinFn, Diagnostic> {
        let builtin = crate::builtins::builtin(name)
            .expect("compiler bug: builtin_call on a name that is not a builtin");
        if builtin.accepts(args.len()) {
            return Ok(builtin);
        }
        Err(self
            .diag(
                span,
                format!(
                    "{name} takes {}, got {}",
                    builtin.arity_phrase(),
                    args.len()
                ),
            )
            .with_headline(ARITY_HEADLINE)
            .with_hint(builtin.hint))
    }

    /// A positional-only arity check (for a nested function reached through the
    /// dispatcher): the argument count must be within the header's accepted range.
    pub(super) fn check_positional_arity(
        &self,
        name: &str,
        params: &Params,
        got: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let too_few = got < params.required();
        let too_many = params.max_positional().is_some_and(|max| got > max);
        if too_few || too_many {
            return Err(self.arity_error(name, params, got, span));
        }
        Ok(())
    }

    /// Map a call's positional and keyword arguments onto a header's binding slots:
    /// fill each parameter from a positional argument, a keyword argument, or its
    /// default; collect any surplus positionals into the variadic. Returns a
    /// `let`-binding prelude (non-empty only when keyword arguments force an
    /// evaluation-order rewrite) and the argument expressions in binding order.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn resolve_args(
        &self,
        callee: &str,
        params: &Params,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        span: Span,
        emit: &Emit,
    ) -> Result<(String, Vec<String>), Diagnostic> {
        let n = params.params.len();
        let has_vararg = params.has_vararg();

        if !has_vararg && args.len() > n {
            return Err(self.arity_error(callee, params, args.len() + kwargs.len(), span));
        }

        if kwargs.is_empty() {
            if args.len() < params.required() {
                return Err(self.arity_error(callee, params, args.len(), span));
            }
            let mut out = Vec::with_capacity(n + has_vararg as usize);
            for i in 0..n {
                if i < args.len() {
                    out.push(self.expr(&args[i], emit)?);
                } else {
                    let default = params.params[i]
                        .default
                        .as_ref()
                        .expect("compiler bug: unfilled required slot without an arity error");
                    out.push(self.expr(default, emit)?);
                }
            }
            if has_vararg {
                let mut extras = Vec::new();
                for arg in &args[n..] {
                    extras.push(self.expr(arg, emit)?);
                }
                out.push(format!("Value::list(vec![{}])", extras.join(", ")));
            }
            return Ok((String::new(), out));
        }

        // Keyword arguments present: evaluate every provided argument (positional
        // then keyword) into a temporary in written order, then arrange the
        // temporaries into binding order so evaluation stays left-to-right.
        let mut temps: Vec<String> = Vec::new();
        let mut slot: Vec<Option<usize>> = vec![None; n];
        let mut vararg_temps: Vec<usize> = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let ti = temps.len();
            temps.push(self.expr(arg, emit)?);
            if i < n {
                slot[i] = Some(ti);
            } else {
                vararg_temps.push(ti);
            }
        }
        for (kname, kexpr) in kwargs {
            let idx = match params.params.iter().position(|p| &p.name == kname) {
                Some(idx) => idx,
                None => {
                    let names_vararg = params.vararg.as_deref() == Some(kname.as_str());
                    let detail = if names_vararg {
                        format!("{kname} is the many parameter and cannot be given by keyword")
                    } else {
                        format!("{callee} has no parameter {kname}")
                    };
                    return Err(self
                        .diag(span, detail)
                        .with_headline("very keyword. much unknown.")
                        .with_hint(params.render(callee)));
                }
            };
            if slot[idx].is_some() {
                return Err(self
                    .diag(span, format!("{callee} got parameter {kname} twice"))
                    .with_headline("very keyword. much repeat.")
                    .with_hint(params.render(callee)));
            }
            let ti = temps.len();
            temps.push(self.expr(kexpr, emit)?);
            slot[idx] = Some(ti);
        }

        let mut out = Vec::with_capacity(n + has_vararg as usize);
        for (i, filled) in slot.iter().enumerate() {
            match filled {
                Some(ti) => out.push(format!("__a{ti}")),
                None => match params.params[i].default.as_ref() {
                    Some(default) => out.push(self.expr(default, emit)?),
                    None => {
                        return Err(self.arity_error(
                            callee,
                            params,
                            args.len() + kwargs.len(),
                            span,
                        ))
                    }
                },
            }
        }
        if has_vararg {
            let items: Vec<String> = vararg_temps.iter().map(|ti| format!("__a{ti}")).collect();
            out.push(format!("Value::list(vec![{}])", items.join(", ")));
        }

        let mut prelude = String::new();
        for (ti, expr) in temps.iter().enumerate() {
            prelude.push_str(&format!("let __a{ti} = {expr}; "));
        }
        Ok((prelude, out))
    }

    /// Assemble a direct call from resolved argument expressions: append `env`,
    /// apply the fail suffix, and wrap in a block when a `let` prelude is present.
    pub(super) fn finish_call(
        &self,
        emit: &Emit,
        prelude: &str,
        mut parts: Vec<String>,
        target: &str,
    ) -> String {
        parts.push("&mut *env".to_string());
        let call = self.fail(emit, format!("{target}({})", parts.join(", ")));
        if prelude.is_empty() {
            call
        } else {
            format!("{{ {prelude}{call} }}")
        }
    }

    /// The arity diagnostic for a call whose count falls outside the header's
    /// accepted range: a fixed count, a `R to X` range, or an `at least R` when a
    /// variadic makes the upper bound unbounded. The hint echoes the call shape.
    pub(super) fn arity_error(
        &self,
        callee: &str,
        params: &Params,
        got: usize,
        span: Span,
    ) -> Diagnostic {
        let required = params.required();
        let phrase = match params.max_positional() {
            Some(max) if max == required => {
                let noun = if required == 1 {
                    "argument"
                } else {
                    "arguments"
                };
                format!("{callee} takes {required} {noun}, got {got}")
            }
            Some(max) => format!("{callee} takes {required} to {max} arguments, got {got}"),
            None => {
                let noun = if required == 1 {
                    "argument"
                } else {
                    "arguments"
                };
                format!("{callee} takes at least {required} {noun}, got {got}")
            }
        };
        self.diag(span, phrase)
            .with_headline(ARITY_HEADLINE)
            .with_hint(params.render(callee))
    }
}
