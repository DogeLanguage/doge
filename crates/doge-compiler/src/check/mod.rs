pub(super) use std::collections::HashSet;

pub(super) use crate::ast::{for_each_child_block, Expr, InterpPart, Script, Stmt};
pub(super) use crate::diagnostics::Diagnostic;
pub(super) use crate::modules::Program;
pub(super) use crate::token::Span;

mod scopes;
mod stmt;
#[cfg(test)]
mod tests;

/// Check every file in a program. Each file is checked in its own scope (a
/// module's functions see only that module's names), and a non-entry module gets
/// the extra "defines things only" rule on top.
pub fn check_program(program: &Program) -> Result<(), Diagnostic> {
    for file in &program.files {
        check(&file.path, &file.source, &file.script)?;
        if !file.is_entry {
            check_module_defs_only(&file.path, &file.source, &file.script)?;
        }
    }
    Ok(())
}

/// A module file may only *define* things — functions, constants, and imports.
/// A loose statement would have to run at import time, which doge modules never
/// do; an object definition in a module lands in a later milestone.
fn check_module_defs_only(path: &str, source: &str, script: &Script) -> Result<(), Diagnostic> {
    let lines = crate::diagnostics::split_source_lines(source);
    let make = |span: Span, message: String| {
        let source_line = crate::diagnostics::source_line(&lines, span.line);
        Diagnostic::new(path, span.line, span.col, source_line, message)
    };

    for stmt in &script.stmts {
        match stmt {
            Stmt::FuncDef { .. } | Stmt::ConstDecl { .. } | Stmt::Import { .. } => {}
            Stmt::ObjDef { name, span, .. } => {
                return Err(make(
                    *span,
                    format!("objects in a module land in a later milestone: {name}"),
                )
                .with_headline("very object. much soon.")
                .with_hint(format!("define {name} in your main script for now")));
            }
            other => {
                return Err(make(
                    other.span(),
                    "a module only defines things — this statement would have to run".to_string(),
                )
                .with_headline("very loose. much module.")
                .with_hint("wrap it in a such …: function, or move it to your main script"));
            }
        }
    }
    Ok(())
}

/// Run every semantic check over `script`. `path`/`source` are only used to
/// render diagnostics against the original text.
pub fn check(path: &str, source: &str, script: &Script) -> Result<(), Diagnostic> {
    let lines = crate::diagnostics::split_source_lines(source);
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
            // Introduce no top-level name; listed explicitly so a new statement
            // that should be a global cannot be silently skipped here.
            Stmt::Assign { .. }
            | Stmt::Bark { .. }
            | Stmt::If { .. }
            | Stmt::For { .. }
            | Stmt::While { .. }
            | Stmt::Try { .. }
            | Stmt::Return { .. }
            | Stmt::Bonk { .. }
            | Stmt::Bork { .. }
            | Stmt::Continue { .. }
            | Stmt::ExprStmt { .. } => {}
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

    /// Is `name` usable at this point? Locals (declared so far) and builtins are
    /// always fine; top-level names are additionally visible inside functions.
    fn in_scope(&self, name: &str, ctx: &Ctx) -> bool {
        crate::builtins::is_builtin(name)
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
        let source_line = crate::diagnostics::source_line(&self.lines, span.line);
        Diagnostic::new(&self.path, span.line, span.col, source_line, message)
    }
}
