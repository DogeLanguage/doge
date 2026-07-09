use std::collections::HashSet;

use crate::ast::{Expr, Script, Stmt};
use crate::diagnostics::Diagnostic;
use crate::token::Span;

/// Builtins always available without an import (mirrors `doge-runtime`).
pub(crate) const BUILTINS: &[&str] = &["len", "str", "int", "float", "range"];

/// Run every semantic check over `script`. `path`/`source` are only used to
/// render diagnostics against the original text.
pub fn check(path: &str, source: &str, script: &Script) -> Result<(), Diagnostic> {
    let lines = source
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    let mut checker = Checker {
        path: path.to_string(),
        lines,
        globals: HashSet::new(),
        consts: HashSet::new(),
    };

    // Pre-pass: every top-level name, and the top-level constants.
    for stmt in &script.stmts {
        match stmt {
            Stmt::Decl { name, .. } | Stmt::FuncDef { name, .. } | Stmt::ObjDef { name, .. } => {
                checker.globals.insert(name.clone());
            }
            Stmt::Import { module, .. } => {
                checker.globals.insert(module.clone());
            }
            Stmt::ConstDecl { name, .. } => {
                checker.globals.insert(name.clone());
                checker.consts.insert(name.clone());
            }
            _ => {}
        }
    }

    checker.check_unique_toplevel(script)?;

    let mut ctx = Ctx {
        locals: HashSet::new(),
        in_function: false,
        loop_depth: 0,
    };
    checker.check_stmts(&script.stmts, &mut ctx)
}

struct Checker {
    path: String,
    lines: Vec<String>,
    /// All top-level names — visible from inside any function body.
    globals: HashSet<String>,
    /// Names bound with `so … =` — reassigning any of them is an error.
    consts: HashSet<String>,
}

/// The scope state threaded through a single function (or the top level).
/// Cloneable so a control-flow branch could be checked in isolation if needed;
/// today branches share the parent scope (names leak, Python-style).
struct Ctx {
    /// Names declared so far in this scope (params, then local declarations).
    locals: HashSet<String>,
    in_function: bool,
    loop_depth: usize,
}

impl Checker {
    fn check_stmts(&mut self, stmts: &[Stmt], ctx: &mut Ctx) -> Result<(), Diagnostic> {
        for stmt in stmts {
            self.check_stmt(stmt, ctx)?;
        }
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &Stmt, ctx: &mut Ctx) -> Result<(), Diagnostic> {
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
                self.check_function(params, body, false)?;
            }
            Stmt::ObjDef { name, methods, .. } => {
                ctx.locals.insert(name.clone());
                for method in methods {
                    if let Stmt::FuncDef { params, body, .. } = method {
                        self.check_function(params, body, true)?;
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
    fn check_unique_toplevel(&self, script: &Script) -> Result<(), Diagnostic> {
        let hoisted = crate::codegen::hoisted_names(&script.stmts);
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

    fn name_clash(&self, span: Span, message: String) -> Diagnostic {
        self.diag(span, message)
            .with_headline("very twice. much name.")
            .with_hint("pick a different name")
    }

    fn method_clash(&self, span: Span, message: String) -> Diagnostic {
        self.diag(span, message)
            .with_headline("very twice. much name.")
            .with_hint("pick a different name for the method")
    }

    /// Check a function or method body in its own fresh scope.
    fn check_function(
        &mut self,
        params: &[String],
        body: &[Stmt],
        is_method: bool,
    ) -> Result<(), Diagnostic> {
        let mut locals = HashSet::new();
        if is_method {
            locals.insert("self".to_string());
        }
        for param in params {
            locals.insert(param.clone());
        }
        let mut ctx = Ctx {
            locals,
            in_function: true,
            loop_depth: 0,
        };
        self.check_stmts(body, &mut ctx)
    }

    /// Verify an assignment target: the name must already be declared, and must
    /// not be a constant. Index/attr targets only require their object to be in
    /// scope (checked as an ordinary read).
    fn check_assign_target(&self, target: &Expr, ctx: &Ctx, span: Span) -> Result<(), Diagnostic> {
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

    fn check_expr(&self, expr: &Expr, ctx: &Ctx) -> Result<(), Diagnostic> {
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
        }
    }

    /// Is `name` usable at this point? Locals (declared so far) and builtins are
    /// always fine; top-level names are additionally visible inside functions.
    fn in_scope(&self, name: &str, ctx: &Ctx) -> bool {
        BUILTINS.contains(&name)
            || ctx.locals.contains(name)
            || (ctx.in_function && self.globals.contains(name))
    }

    fn undeclared_assign(&self, name: &str, span: Span) -> Diagnostic {
        self.diag(
            span,
            format!("cannot assign to {name} before it is declared"),
        )
        .with_headline("very undeclared. much assign.")
        .with_hint(format!("declare it first — such {name} = …"))
    }

    fn diag(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        let source_line = self
            .lines
            .get((span.line as usize).saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        Diagnostic::new(&self.path, span.line, span.col, source_line, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn check_src(source: &str) -> Result<(), Diagnostic> {
        let script = parse("test.doge", source).expect("parse should succeed");
        check("test.doge", source, &script)
    }

    #[test]
    fn clean_program_passes() {
        assert!(check_src("such x = 1\nbark x\nwow\n").is_ok());
    }

    #[test]
    fn assign_to_undeclared_is_an_error() {
        let err = check_src("x = 1\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very undeclared. much assign.");
    }

    #[test]
    fn very_assign_to_undeclared_is_an_error() {
        let err = check_src("very x = 1\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very undeclared. much assign.");
    }

    #[test]
    fn reassigning_a_const_is_an_error() {
        let err = check_src("so PI = 3\nPI = 4\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very const. much fixed.");
    }

    #[test]
    fn reading_an_undeclared_name_is_an_error() {
        let err = check_src("bark nope\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very unknown. much name.");
    }

    #[test]
    fn bork_outside_loop_is_an_error() {
        let err = check_src("bork\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very bork. much nowhere.");
    }

    #[test]
    fn continue_outside_loop_is_an_error() {
        let err = check_src("continue\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very continue. much nowhere.");
    }

    #[test]
    fn return_outside_function_is_an_error() {
        let err = check_src("return 1\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very return. much lost.");
    }

    #[test]
    fn bork_inside_loop_is_fine() {
        assert!(check_src("such xs = [1]\nfor x in xs:\n    bork\nwow\n").is_ok());
    }

    #[test]
    fn return_inside_function_is_fine() {
        assert!(check_src("such f:\n    return 1\nwow\nwow\n").is_ok());
    }

    #[test]
    fn mutual_recursion_is_allowed() {
        // `a` calls `b`, defined later; both are top-level names via the pre-pass.
        let src = "such a:\n    b()\nwow\nsuch b:\n    a()\nwow\nwow\n";
        assert!(check_src(src).is_ok());
    }

    #[test]
    fn params_and_self_are_in_scope() {
        let func = "such greet much name:\n    bark name\nwow\nwow\n";
        assert!(check_src(func).is_ok());
        let method = "many Shibe:\n    such speak:\n        bark self\n    wow\nwow\nwow\n";
        assert!(check_src(method).is_ok());
    }

    #[test]
    fn builtin_names_are_known() {
        assert!(check_src("such xs = [1]\nbark len(xs)\nwow\n").is_ok());
    }

    #[test]
    fn duplicate_function_names_are_an_error() {
        let err =
            check_src("such f:\n    bark 1\nwow\nsuch f:\n    bark 2\nwow\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very twice. much name.");
    }

    #[test]
    fn function_clashing_with_a_variable_is_an_error() {
        let err = check_src("such x = 1\nsuch x:\n    bark 1\nwow\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very twice. much name.");
    }

    #[test]
    fn function_named_like_a_builtin_is_an_error() {
        let err = check_src("such len:\n    bark 1\nwow\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very twice. much name.");
        assert!(err.message.contains("builtin"));
    }

    #[test]
    fn duplicate_class_names_are_an_error() {
        let err =
            check_src("many Shibe:\n    such a:\n        bark 1\n    wow\nwow\nmany Shibe:\n    such b:\n        bark 2\n    wow\nwow\nwow\n")
                .unwrap_err();
        assert_eq!(err.headline, "very twice. much name.");
    }

    #[test]
    fn class_named_like_a_builtin_is_an_error() {
        let err =
            check_src("many len:\n    such a:\n        bark 1\n    wow\nwow\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very twice. much name.");
        assert!(err.message.contains("builtin"));
    }

    #[test]
    fn import_clashing_with_a_variable_is_an_error() {
        let err = check_src("such nerd = 1\nso nerd\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very twice. much name.");
    }

    #[test]
    fn importing_the_same_module_twice_is_an_error() {
        let err = check_src("so nerd\nso nerd\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very twice. much name.");
    }

    #[test]
    fn duplicate_method_in_one_class_is_an_error() {
        let err = check_src(
            "many Shibe:\n    such speak:\n        bark 1\n    wow\n    such speak:\n        bark 2\n    wow\nwow\nwow\n",
        )
        .unwrap_err();
        assert_eq!(err.headline, "very twice. much name.");
        assert!(err.message.contains("method"));
    }

    #[test]
    fn top_level_use_before_declaration_is_an_error() {
        // `y` is a top-level name, but used before its declaration line.
        let err = check_src("bark y\nsuch y = 1\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very unknown. much name.");
    }
}
