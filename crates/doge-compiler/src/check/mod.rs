pub(super) use std::collections::{HashMap, HashSet};

pub(super) use crate::ast::{for_each_child_block, Expr, InterpPart, Params, Script, Stmt};
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

/// A module file may only *define* things — functions, objects, constants, and
/// imports. A loose statement would have to run at import time, which doge modules
/// never do.
fn check_module_defs_only(path: &str, source: &str, script: &Script) -> Result<(), Diagnostic> {
    let lines = crate::diagnostics::split_source_lines(source);
    let make = |span: Span, message: String| {
        let source_line = crate::diagnostics::source_line(&lines, span.line);
        Diagnostic::new(path, span.line, span.col, source_line, message)
    };

    for stmt in &script.stmts {
        match stmt {
            Stmt::FuncDef { .. }
            | Stmt::ObjDef { .. }
            | Stmt::ConstDecl { .. }
            | Stmt::Import { .. } => {}
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
        classes: HashMap::new(),
    };

    // Pre-pass: every top-level name, and the top-level constants.
    for stmt in &script.stmts {
        match stmt {
            Stmt::FuncDef { name, .. } => {
                checker.globals.insert(name.clone());
            }
            Stmt::ObjDef {
                name,
                parent,
                methods,
                span,
            } => {
                checker.globals.insert(name.clone());
                let method_names = methods
                    .iter()
                    .filter_map(|m| match m {
                        Stmt::FuncDef { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                    .collect();
                checker.classes.insert(
                    name.clone(),
                    ClassSig {
                        parent: parent.clone(),
                        methods: method_names,
                        span: *span,
                    },
                );
            }
            Stmt::Decl { names, rest, .. } => {
                for name in names {
                    checker.globals.insert(name.clone());
                }
                if let Some(rest) = rest {
                    checker.globals.insert(rest.clone());
                }
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
            | Stmt::Amaze { .. }
            | Stmt::Bork { .. }
            | Stmt::Continue { .. }
            | Stmt::ExprStmt { .. } => {}
        }
    }

    checker.check_unique_toplevel(script)?;
    checker.check_inheritance()?;

    let mut ctx = Ctx {
        locals: HashSet::new(),
        in_function: false,
        class: None,
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
    /// Object definitions in this file, by name — their parent and method names,
    /// for validating the inheritance graph and `super` calls.
    classes: HashMap<String, ClassSig>,
}

/// One class's inheritance-relevant signature: the parent it names (if any) and
/// the method names it defines directly.
struct ClassSig {
    parent: Option<String>,
    methods: HashSet<String>,
    span: Span,
}

/// The scope state threaded through a single function (or the top level).
/// Cloneable so a control-flow branch could be checked in isolation if needed;
/// today branches share the parent scope (names leak, Python-style).
struct Ctx {
    /// Names declared so far in this scope (params, then local declarations).
    locals: HashSet<String>,
    in_function: bool,
    /// The class whose method body is being checked, if any — set only for a
    /// direct method body, so `super` is rejected in a plain function or a closure.
    class: Option<String>,
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

    /// Validate the inheritance graph: every named parent is a class defined in
    /// this file, and no class is its own ancestor. A parent in another file is not
    /// supported, so it reads here as an unknown parent.
    fn check_inheritance(&self) -> Result<(), Diagnostic> {
        let mut names: Vec<&String> = self.classes.keys().collect();
        names.sort();
        for name in &names {
            let sig = &self.classes[*name];
            if let Some(parent) = &sig.parent {
                if !self.classes.contains_key(parent) {
                    return Err(self
                        .diag(
                            sig.span,
                            format!("{name} inherits from {parent}, which is not a class here"),
                        )
                        .with_headline("very parent. much unknown.")
                        .with_hint(format!(
                            "define many {parent}: in this file, or fix the name"
                        )));
                }
            }
        }
        // Cycle detection: from each class, walk up the chain; returning to the
        // start is a loop. The guard bounds a cycle that does not include the start
        // (a later iteration reports it anchored at one of its own members).
        for name in &names {
            let mut chain = vec![name.as_str()];
            let mut cur = self.classes[*name].parent.as_deref();
            let mut guard = 0;
            while let Some(c) = cur {
                chain.push(c);
                if c == name.as_str() {
                    let sig = &self.classes[*name];
                    return Err(self
                        .diag(
                            sig.span,
                            format!("these classes inherit in a loop: {}", chain.join(" → ")),
                        )
                        .with_headline("very loop. much family.")
                        .with_hint("break the cycle — a class cannot be its own ancestor"));
                }
                guard += 1;
                if guard > self.classes.len() {
                    break;
                }
                cur = self.classes.get(c).and_then(|s| s.parent.as_deref());
            }
        }
        Ok(())
    }

    /// Validate a `super.method(…)` call: it must be inside a method whose class
    /// has a parent, and some ancestor must define `method`.
    fn check_super(&self, method: &str, ctx: &Ctx, span: Span) -> Result<(), Diagnostic> {
        let Some(class) = &ctx.class else {
            return Err(self
                .diag(span, "super only works inside a method")
                .with_headline("very super. much lost.")
                .with_hint("use super inside a method of a class with a parent"));
        };
        let sig = self
            .classes
            .get(class)
            .expect("compiler bug: a method's class is always known");
        let Some(parent) = &sig.parent else {
            return Err(self
                .diag(span, format!("{class} has no parent to call super on"))
                .with_headline("very super. much orphan.")
                .with_hint(format!("give it a parent — many {class} much Parent:")));
        };
        let mut cur = Some(parent.as_str());
        let mut guard = 0;
        while let Some(c) = cur {
            let Some(csig) = self.classes.get(c) else {
                break;
            };
            if csig.methods.contains(method) {
                return Ok(());
            }
            guard += 1;
            if guard > self.classes.len() {
                break;
            }
            cur = csig.parent.as_deref();
        }
        Err(self
            .diag(span, format!("no parent of {class} has a method {method}"))
            .with_headline("very super. much unknown.")
            .with_hint(format!("check the method name — super.{method}(…)")))
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
