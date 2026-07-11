use super::*;

impl Codegen {
    pub(super) fn call(
        &self,
        callee: &Expr,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        match callee {
            Expr::Ident { name, .. } => {
                // A local shadows every other meaning — call through its value. A
                // known nested function still gets a compile-time arity check.
                if emit.locals.contains_key(name) {
                    if let Some(params) = emit.local_funcs.get(name) {
                        self.check_user_arity(name, params, args, span)?;
                    }
                    let callee_val = self.resolve_read(emit, name);
                    return self.indirect_call(&callee_val, args, emit);
                }
                if BUILTINS.contains(&name.as_str()) {
                    return self.builtin_call(name, args, span, emit);
                }
                if let Some(params) = emit.table().funcs.get(name) {
                    self.check_user_arity(name, params, args, span)?;
                    let mut parts = Vec::with_capacity(args.len() + 1);
                    for arg in args {
                        parts.push(self.expr(arg, emit)?);
                    }
                    parts.push("&mut *env".to_string());
                    let call =
                        format!("{}({})", func_wrapper(emit.file_id, name), parts.join(", "));
                    return Ok(self.fail(emit, call));
                }
                // `Shibe(...)` — a constructor, resolved statically.
                if emit.class(name).is_some() {
                    return self.constructor_call(name, args, span, emit);
                }
                // A module name is not itself callable — you call one of its members.
                if let Some(member) = self.module_first_member(emit, name) {
                    return Err(self
                        .diag(span, format!("{name} is a module, not a function"))
                        .with_headline("very module. much confuse.")
                        .with_hint(format!("call a member — {name}.{member}(…)")));
                }
                // A top-level variable holding a function value: call it indirectly.
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
                    self.module_call(base, module, name, args, span, emit)
                } else {
                    let fid = emit
                        .user_module(base)
                        .expect("compiler bug: module vanished");
                    self.user_module_call(base, fid, name, args, span, emit)
                }
            }
            // `kabosu.speak(...)` — a method call, dispatched at runtime.
            Expr::Attr { obj, name, .. } => {
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
                let callee_val = self.expr(callee, emit)?;
                self.indirect_call(&callee_val, args, emit)
            }
        }
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

    /// A constructor call `Shibe(args)`: static arity against `init`, then a
    /// `n_<id>(args…, &mut *env)` through the fail suffix.
    pub(super) fn constructor_call(
        &self,
        name: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        let class = emit
            .class(name)
            .expect("compiler bug: constructor for an unknown class");
        let init_params = class.init_params();
        if args.len() != init_params.len() {
            let count = init_params.len();
            let noun = if count == 1 { "argument" } else { "arguments" };
            let hint = if init_params.is_empty() {
                format!("{name}()")
            } else {
                format!("{name}({})", init_params.join(", "))
            };
            return Err(self
                .diag(
                    span,
                    format!("{name} takes {count} {noun}, got {}", args.len()),
                )
                .with_headline(ARITY_HEADLINE)
                .with_hint(hint));
        }
        let mut parts = Vec::with_capacity(args.len() + 1);
        for arg in args {
            parts.push(self.expr(arg, emit)?);
        }
        parts.push("&mut *env".to_string());
        let call = format!("{CTOR_PREFIX}{}({})", class.id, parts.join(", "));
        Ok(self.fail(emit, call))
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

    /// A user module member call `utils.square(args)`: static arity against the
    /// module's function table, then a direct call to its mangled wrapper.
    /// Calling a constant, or an unknown member, is a real error.
    pub(super) fn user_module_call(
        &self,
        module_name: &str,
        fid: u32,
        member: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
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
        if args.len() != params.len() {
            let noun = if params.len() == 1 {
                "argument"
            } else {
                "arguments"
            };
            let call_shape = if params.is_empty() {
                format!("{module_name}.{member}()")
            } else {
                format!("{module_name}.{member}({})", params.join(", "))
            };
            return Err(self
                .diag(
                    span,
                    format!(
                        "{module_name}.{member} takes {} {noun}, got {}",
                        params.len(),
                        args.len()
                    ),
                )
                .with_headline(ARITY_HEADLINE)
                .with_hint(call_shape));
        }
        let mut parts = Vec::with_capacity(args.len() + 1);
        for arg in args {
            parts.push(self.expr(arg, emit)?);
        }
        parts.push("&mut *env".to_string());
        let call = format!("{}({})", func_wrapper(fid, member), parts.join(", "));
        Ok(self.fail(emit, call))
    }

    pub(super) fn builtin_call(
        &self,
        name: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        self.check_builtin_arity(name, args, span)?;
        match name {
            "len" => Ok(self.fail(emit, format!("len(&{})", self.expr(&args[0], emit)?))),
            "str" => Ok(format!("to_str(&{})", self.expr(&args[0], emit)?)),
            "int" => Ok(self.fail(emit, format!("to_int(&{})", self.expr(&args[0], emit)?))),
            "float" => Ok(self.fail(emit, format!("to_float(&{})", self.expr(&args[0], emit)?))),
            "range" if args.len() == 1 => Ok(self.fail(
                emit,
                format!("range(&Value::Int(0i64), &{})", self.expr(&args[0], emit)?),
            )),
            "range" => Ok(self.fail(
                emit,
                format!(
                    "range(&{}, &{})",
                    self.expr(&args[0], emit)?,
                    self.expr(&args[1], emit)?
                ),
            )),
            _ => unreachable!("compiler bug: arity check admitted a non-builtin"),
        }
    }

    /// Builtin arity is statically known: `len`/`str`/`int`/`float` take one
    /// argument, `range` takes one or two.
    pub(super) fn check_builtin_arity(
        &self,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let (ok, expects, hint) = match name {
            "range" => (
                args.len() == 1 || args.len() == 2,
                "1 or 2 arguments",
                "range(n) or range(a, b)".to_string(),
            ),
            _ => (args.len() == 1, "1 argument", format!("{name}(thing)")),
        };
        if ok {
            return Ok(());
        }
        Err(self
            .diag(span, format!("{name} takes {expects}, got {}", args.len()))
            .with_headline(ARITY_HEADLINE)
            .with_hint(hint))
    }

    /// A user function takes exactly its declared parameters; the hint echoes the
    /// call shape doge expected.
    pub(super) fn check_user_arity(
        &self,
        name: &str,
        params: &[String],
        args: &[Expr],
        span: Span,
    ) -> Result<(), Diagnostic> {
        if args.len() == params.len() {
            return Ok(());
        }
        let count = params.len();
        let noun = if count == 1 { "argument" } else { "arguments" };
        let hint = if params.is_empty() {
            format!("{name}()")
        } else {
            format!("{name}({})", params.join(", "))
        };
        Err(self
            .diag(
                span,
                format!("{name} takes {count} {noun}, got {}", args.len()),
            )
            .with_headline(ARITY_HEADLINE)
            .with_hint(hint))
    }
}
