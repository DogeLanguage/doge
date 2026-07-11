use super::scopes::*;
use super::*;

impl Checker {
    pub(super) fn check_stmts(&mut self, stmts: &[Stmt], ctx: &mut Ctx) -> Result<(), Diagnostic> {
        for stmt in stmts {
            self.check_stmt(stmt, ctx)?;
        }
        Ok(())
    }

    pub(super) fn check_stmt(&mut self, stmt: &Stmt, ctx: &mut Ctx) -> Result<(), Diagnostic> {
        match stmt {
            Stmt::Decl { name, expr, .. } => {
                self.check_expr(expr, ctx)?;
                ctx.locals.insert(name.clone());
            }
            Stmt::ConstDecl { name, expr, .. } => {
                self.check_expr(expr, ctx)?;
                ctx.locals.insert(name.clone());
                self.consts.insert(name.clone());
            }
            Stmt::Import { module, .. } => {
                ctx.locals.insert(module.clone());
            }
            Stmt::Assign {
                target, expr, span, ..
            } => {
                self.check_expr(expr, ctx)?;
                self.check_assign_target(target, ctx, *span)?;
            }
            Stmt::Bark { expr, .. } => self.check_expr(expr, ctx)?,
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (cond, body) in branches {
                    self.check_expr(cond, ctx)?;
                    self.check_stmts(body, ctx)?;
                }
                if let Some(body) = else_body {
                    self.check_stmts(body, ctx)?;
                }
            }
            Stmt::For {
                var, iter, body, ..
            } => {
                self.check_expr(iter, ctx)?;
                ctx.locals.insert(var.clone());
                ctx.loop_depth += 1;
                let result = self.check_stmts(body, ctx);
                ctx.loop_depth -= 1;
                result?;
            }
            Stmt::While { cond, body, .. } => {
                self.check_expr(cond, ctx)?;
                ctx.loop_depth += 1;
                let result = self.check_stmts(body, ctx);
                ctx.loop_depth -= 1;
                result?;
            }
            Stmt::FuncDef {
                name, params, body, ..
            } => {
                ctx.locals.insert(name.clone());
                // A nested function sees the enclosing function's locals; a
                // top-level function only sees globals (added inside check_function).
                let enclosing = if ctx.in_function {
                    ctx.locals.clone()
                } else {
                    HashSet::new()
                };
                self.check_function(params, body, false, &enclosing)?;
            }
            Stmt::ObjDef { name, methods, .. } => {
                ctx.locals.insert(name.clone());
                for method in methods {
                    if let Stmt::FuncDef { params, body, .. } = method {
                        self.check_function(params, body, true, &HashSet::new())?;
                    }
                }
            }
            Stmt::Try {
                body,
                err_name,
                handler,
                ..
            } => {
                self.check_stmts(body, ctx)?;
                ctx.locals.insert(err_name.clone());
                self.check_stmts(handler, ctx)?;
            }
            Stmt::Return { expr, span } => {
                if !ctx.in_function {
                    return Err(self
                        .diag(*span, "return only makes sense inside a function")
                        .with_headline("very return. much lost.")
                        .with_hint("put this return inside a such …: function"));
                }
                if let Some(expr) = expr {
                    self.check_expr(expr, ctx)?;
                }
            }
            Stmt::Bonk { expr, .. } => self.check_expr(expr, ctx)?,
            Stmt::Bork { span } => {
                if ctx.loop_depth == 0 {
                    return Err(self
                        .diag(*span, "bork can only break out of a loop")
                        .with_headline("very bork. much nowhere.")
                        .with_hint("use bork inside a for or while loop"));
                }
            }
            Stmt::Continue { span } => {
                if ctx.loop_depth == 0 {
                    return Err(self
                        .diag(*span, "continue can only skip within a loop")
                        .with_headline("very continue. much nowhere.")
                        .with_hint("use continue inside a for or while loop"));
                }
            }
            Stmt::ExprStmt { expr } => self.check_expr(expr, ctx)?,
        }
        Ok(())
    }

    /// Every top-level definition — function, object, or import — introduces a
    /// name that must be unique: not a builtin, not another definition, and not a
    /// hoisted variable/loop/error name. Within an object, method names must be
    /// unique too.
    pub(super) fn check_unique_toplevel(&self, script: &Script) -> Result<(), Diagnostic> {
        let hoisted = crate::codegen::toplevel_hoisted(&script.stmts);
        let hoisted: HashSet<&str> = hoisted.iter().map(String::as_str).collect();

        let mut seen: HashSet<&str> = HashSet::new();
        for stmt in &script.stmts {
            let (name, span) = match stmt {
                Stmt::FuncDef { name, span, .. } | Stmt::ObjDef { name, span, .. } => {
                    (name.as_str(), *span)
                }
                Stmt::Import { module, span, .. } => (module.as_str(), *span),
                _ => continue,
            };
            if BUILTINS.contains(&name) {
                return Err(self.name_clash(span, format!("{name} is already a builtin")));
            }
            if seen.contains(name) || hoisted.contains(name) {
                return Err(self.name_clash(span, format!("{name} is already defined")));
            }
            seen.insert(name);

            if let Stmt::ObjDef {
                name: class,
                methods,
                ..
            } = stmt
            {
                let mut method_seen: HashSet<&str> = HashSet::new();
                for method in methods {
                    if let Stmt::FuncDef {
                        name: method_name,
                        span: method_span,
                        ..
                    } = method
                    {
                        if !method_seen.insert(method_name.as_str()) {
                            return Err(self.method_clash(
                                *method_span,
                                format!("{class} already has a method {method_name}"),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Check a function or method body. Its scope starts from the enclosing
    /// function's locals (empty for a top-level function or method, which only
    /// see globals), plus `self`, its parameters, and every nested-function name
    /// it defines — the last so siblings can call each other regardless of order.
    pub(super) fn check_function(
        &mut self,
        params: &[String],
        body: &[Stmt],
        is_method: bool,
        enclosing: &HashSet<String>,
    ) -> Result<(), Diagnostic> {
        self.check_scope_collisions(params, body)?;

        let mut locals = enclosing.clone();
        if is_method {
            locals.insert("self".to_string());
        }
        for param in params {
            locals.insert(param.clone());
        }
        for name in nested_func_names(body) {
            locals.insert(name);
        }
        let mut ctx = Ctx {
            locals,
            in_function: true,
            loop_depth: 0,
        };
        self.check_stmts(body, &mut ctx)
    }

    /// A nested-function name is a fixed binding: it may not repeat, clash with a
    /// parameter, or clash with a variable/loop/error name in the same body.
    pub(super) fn check_scope_collisions(
        &self,
        params: &[String],
        body: &[Stmt],
    ) -> Result<(), Diagnostic> {
        let mut others: HashSet<String> = params.iter().cloned().collect();
        collect_var_bindings(body, &mut others);
        let mut seen: HashSet<&str> = HashSet::new();
        for (name, span) in nested_funcs_with_span(body) {
            if BUILTINS.contains(&name) {
                return Err(self.name_clash(span, format!("{name} is already a builtin")));
            }
            if others.contains(name) || !seen.insert(name) {
                return Err(self.name_clash(span, format!("{name} is already defined")));
            }
        }
        Ok(())
    }

    /// Verify an assignment target: the name must already be declared, and must
    /// not be a constant. Index/attr targets only require their object to be in
    /// scope (checked as an ordinary read).
    pub(super) fn check_assign_target(
        &self,
        target: &Expr,
        ctx: &Ctx,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match target {
            Expr::Ident { name, .. } => {
                if self.consts.contains(name) {
                    return Err(self
                        .diag(
                            span,
                            format!("cannot reassign so {name} — it is a constant"),
                        )
                        .with_headline("very const. much fixed.")
                        .with_hint(format!("pick a new name, or declare {name} with such")));
                }
                if !self.in_scope(name, ctx) {
                    return Err(self.undeclared_assign(name, span));
                }
                Ok(())
            }
            // `xs[0] = …` / `x.name = …`: the object is read, so check it.
            Expr::Index { .. } | Expr::Attr { .. } => self.check_expr(target, ctx),
            // The parser already guaranteed a valid target shape.
            _ => Ok(()),
        }
    }

    pub(super) fn check_expr(&self, expr: &Expr, ctx: &Ctx) -> Result<(), Diagnostic> {
        match expr {
            Expr::Int { .. }
            | Expr::Float { .. }
            | Expr::Str { .. }
            | Expr::Bool { .. }
            | Expr::None { .. } => Ok(()),
            Expr::Ident { name, span } => {
                if self.in_scope(name, ctx) {
                    Ok(())
                } else {
                    Err(self
                        .diag(*span, format!("doge does not know the name {name}"))
                        .with_headline("very unknown. much name.")
                        .with_hint(format!("declare it first — such {name} = …")))
                }
            }
            Expr::List { items, .. } => {
                for item in items {
                    self.check_expr(item, ctx)?;
                }
                Ok(())
            }
            Expr::Dict { entries, .. } => {
                for (key, value) in entries {
                    self.check_expr(key, ctx)?;
                    self.check_expr(value, ctx)?;
                }
                Ok(())
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr(lhs, ctx)?;
                self.check_expr(rhs, ctx)
            }
            Expr::Unary { operand, .. } => self.check_expr(operand, ctx),
            Expr::Call { callee, args, .. } => {
                self.check_expr(callee, ctx)?;
                for arg in args {
                    self.check_expr(arg, ctx)?;
                }
                Ok(())
            }
            Expr::Index { obj, index, .. } => {
                self.check_expr(obj, ctx)?;
                self.check_expr(index, ctx)
            }
            // Attribute names are dynamic — only the object is a name to resolve.
            Expr::Attr { obj, .. } => self.check_expr(obj, ctx),
            Expr::StrInterp { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(hole) = part {
                        self.check_expr(hole, ctx)?;
                    }
                }
                Ok(())
            }
        }
    }
}
