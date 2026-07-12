use super::*;

impl Codegen {
    /// Codegen an expression to a Rust expression string. Every fallible runtime
    /// call is routed through [`Codegen::fail`].
    pub(super) fn expr(&self, expr: &Expr, emit: &Emit) -> Result<String, Diagnostic> {
        match expr {
            Expr::Int { value, .. } => Ok(format!("Value::Int({value}i64)")),
            Expr::Float { value, .. } => Ok(format!("Value::Float({value:?}f64)")),
            Expr::Str { value, .. } => Ok(format!("Value::str(\"{}\")", escape_str(value))),
            Expr::Bool { value, .. } => Ok(format!("Value::Bool({value})")),
            Expr::None { .. } => Ok("Value::None".to_string()),
            Expr::Ident { name, span } => {
                if emit.locals.contains_key(name) {
                    // A nested-function name is a `Cell` holding its function
                    // value, so reading it yields that value — no special case.
                    Ok(self.resolve_read(emit, name))
                } else if let Some(id) = emit.func_value_id(name) {
                    emit.materialized.borrow_mut().insert(id);
                    Ok(format!(
                        "Value::function({id}u32, \"{}\", vec![])",
                        escape_str(name)
                    ))
                } else if emit.class(name).is_some() {
                    // A class name as a value: a callable that builds an instance,
                    // materialized as a `Value::class` over its constructor arm.
                    let id = emit.analysis.ctor_ids[&(emit.file_id, name.clone())];
                    emit.materialized.borrow_mut().insert(id);
                    Ok(format!("Value::class({id}u32, \"{}\")", escape_str(name)))
                } else if let Some(member) = self.module_first_member(emit, name) {
                    Err(self
                        .diag(*span, format!("{name} is a module, not a value"))
                        .with_headline("very module. much confuse.")
                        .with_hint(format!("use a member — {name}.{member}(…)")))
                } else {
                    Ok(format!("env.{}.clone()", field_name(emit.file_id, name)))
                }
            }
            Expr::List { items, .. } => {
                let mut parts = Vec::with_capacity(items.len());
                for item in items {
                    parts.push(self.expr(item, emit)?);
                }
                Ok(format!("Value::list(vec![{}])", parts.join(", ")))
            }
            Expr::Dict { entries, .. } => {
                let mut pairs = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    pairs.push(format!(
                        "({}, {})",
                        self.expr(key, emit)?,
                        self.expr(value, emit)?
                    ));
                }
                Ok(self.fail(
                    emit,
                    format!("Value::dict_from_pairs(vec![{}])", pairs.join(", ")),
                ))
            }
            Expr::Binary { op, lhs, rhs, .. } => self.binary(*op, lhs, rhs, emit),
            Expr::Unary { op, operand, .. } => {
                let inner = self.expr(operand, emit)?;
                Ok(match op {
                    UnOp::Neg => self.fail(emit, format!("neg({inner})")),
                    UnOp::Not => self.fail(emit, format!("not_({inner})")),
                    UnOp::BitNot => self.fail(emit, format!("bitnot({inner})")),
                })
            }
            Expr::Index { obj, index, .. } => {
                let call = format!(
                    "index_get(&{}, &{})",
                    self.expr(obj, emit)?,
                    self.expr(index, emit)?
                );
                Ok(self.fail(emit, call))
            }
            Expr::Slice {
                obj,
                start,
                end,
                step,
                ..
            } => {
                let part = |part: &Option<Box<Expr>>, emit: &Emit| match part {
                    Some(expr) => self.expr(expr, emit),
                    None => Ok("Value::None".to_string()),
                };
                let call = format!(
                    "slice_get(&{}, &{}, &{}, &{})",
                    self.expr(obj, emit)?,
                    part(start, emit)?,
                    part(end, emit)?,
                    part(step, emit)?
                );
                Ok(self.fail(emit, call))
            }
            Expr::Ternary {
                cond,
                then,
                otherwise,
                ..
            } => {
                // Only the taken branch is evaluated, so both arms live inside the
                // `if`, mirroring the short-circuit `and`/`or` shape in `binary`.
                let c = self.expr(cond, emit)?;
                let t = self.expr(then, emit)?;
                let e = self.expr(otherwise, emit)?;
                Ok(format!("{{ if ({c}).truthy() {{ {t} }} else {{ {e} }} }}"))
            }
            Expr::Call {
                callee,
                args,
                kwargs,
                span,
            } => self.call(callee, args, kwargs, *span, emit),
            Expr::Attr { obj, name, span } => {
                // `module.member` as a value: a const inlines, a function becomes
                // a first-class function value, an unknown member is a real error.
                if let Expr::Ident { name: base, .. } = obj.as_ref() {
                    if let Some(module) = emit.module(base) {
                        if let Some(const_expr) = module.const_expr(name) {
                            return Ok(const_expr.to_string());
                        }
                        if module.func(name).is_some() {
                            let id = emit.analysis.module_fn_ids[&(base.clone(), name.clone())];
                            emit.materialized.borrow_mut().insert(id);
                            return Ok(format!(
                                "Value::function({id}u32, \"{base}.{name}\", vec![])"
                            ));
                        }
                        return Err(self.unknown_member(base, name, module, *span));
                    }
                    if let Some(fid) = emit.user_module(base) {
                        if emit.class_in(fid, name).is_some() {
                            // `utils.Shibe` as a value: the module's constructor arm,
                            // sharing its id so it equals the module's own `Shibe`.
                            let id = emit.analysis.ctor_ids[&(fid, name.clone())];
                            emit.materialized.borrow_mut().insert(id);
                            return Ok(format!("Value::class({id}u32, \"{base}.{name}\")"));
                        }
                        let table = &emit.tables[fid as usize];
                        if table.consts.contains(name) {
                            return Ok(format!("env.{}.clone()", field_name(fid, name)));
                        }
                        if table.funcs.contains_key(name) {
                            let id = emit.analysis.top_func_ids[&(fid, name.clone())];
                            emit.materialized.borrow_mut().insert(id);
                            return Ok(format!(
                                "Value::function({id}u32, \"{base}.{name}\", vec![])"
                            ));
                        }
                        return Err(self.unknown_user_member(emit, base, fid, name, *span));
                    }
                }
                // A bare `obj.name` value read binds a method when there is no
                // field of that name (`such f = a.speak`); `class_has_method` is
                // the gate for object receivers, collections gate themselves.
                emit.uses_attr_read.set(true);
                let call = format!(
                    "attr_get_or_bind(&{}, \"{}\", &class_has_method)",
                    self.expr(obj, emit)?,
                    escape_str(name)
                );
                Ok(self.fail(emit, call))
            }
            Expr::SuperCall { method, args, span } => self.super_call(method, args, *span, emit),
            Expr::StrInterp { parts, .. } => {
                let mut pieces = Vec::with_capacity(parts.len());
                for part in parts {
                    pieces.push(match part {
                        InterpPart::Lit(text) => format!("Value::str(\"{}\")", escape_str(text)),
                        InterpPart::Expr(hole) => self.expr(hole, emit)?,
                    });
                }
                Ok(format!("interp(&[{}])", pieces.join(", ")))
            }
        }
    }

    /// The first member (function then constant) of the module imported as
    /// `name` in the current file — stdlib or user — or `None` if `name` is not
    /// a module there. Drives the "module, not a value/function" hints.
    pub(super) fn module_first_member(&self, emit: &Emit, name: &str) -> Option<String> {
        if let Some(module) = emit.module(name) {
            return Some(module.first_member().to_string());
        }
        let fid = emit.user_module(name)?;
        Some(
            emit.tables[fid as usize]
                .members
                .first()
                .cloned()
                .unwrap_or_default(),
        )
    }

    /// The "module has no member" diagnostic, listing the members it does have.
    pub(super) fn unknown_member(
        &self,
        module_name: &str,
        member: &str,
        module: &Module,
        span: Span,
    ) -> Diagnostic {
        self.diag(span, format!("{module_name} has no member {member}"))
            .with_headline("very module. much unknown.")
            .with_hint(format!("{module_name} has: {}", module.members()))
    }

    /// The "module has no member" diagnostic for a user module.
    pub(super) fn unknown_user_member(
        &self,
        emit: &Emit,
        module_name: &str,
        fid: u32,
        member: &str,
        span: Span,
    ) -> Diagnostic {
        let members = emit.tables[fid as usize].members.join(", ");
        self.diag(span, format!("{module_name} has no member {member}"))
            .with_headline("very module. much unknown.")
            .with_hint(format!("{module_name} has: {members}"))
    }

    pub(super) fn binary(
        &self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        // `and`/`or` are Rust block expressions with the right operand INSIDE the
        // guard, so it is evaluated only when the left operand doesn't decide the
        // result. Both always yield a Bool (Doge rule, docs/SYNTAX.md §3).
        if matches!(op, BinOp::And | BinOp::Or) {
            let l = self.expr(lhs, emit)?;
            let r = self.expr(rhs, emit)?;
            return Ok(match op {
                BinOp::And => format!(
                    "{{ let l = {l}; if !l.truthy() {{ Value::Bool(false) }} else {{ Value::Bool(({r}).truthy()) }} }}"
                ),
                BinOp::Or => format!(
                    "{{ let l = {l}; if l.truthy() {{ Value::Bool(true) }} else {{ Value::Bool(({r}).truthy()) }} }}"
                ),
                _ => unreachable!(),
            });
        }
        let l = self.expr(lhs, emit)?;
        let r = self.expr(rhs, emit)?;
        Ok(self.fail(emit, format!("{}({l}, {r})", binop_call(op))))
    }
}
