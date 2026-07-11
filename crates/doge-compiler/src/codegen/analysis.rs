//! Whole-program function/capture analysis and scope-name collection.

use std::collections::{HashMap, HashSet};

use crate::ast::{Expr, InterpPart, Stmt};
use crate::check::BUILTINS;
use crate::modules::{Program, ProgramFile};
use crate::stdlib::Module;
use crate::token::Span;

/// A file's top-level names, gathered once so any file's code can resolve calls,
/// constants, and members into another file. Indexed program-wide by `file_id`.
pub(super) struct FileTable {
    /// Top-level function name → its parameter names.
    pub(super) funcs: HashMap<String, Vec<String>>,
    /// Top-level constant names.
    pub(super) consts: HashSet<String>,
    /// Public member names (functions then constants) in source order, for hints.
    pub(super) members: Vec<String>,
    /// This file's imports: local name → stdlib table entry.
    pub(super) stdlib_imports: HashMap<String, &'static Module>,
    /// This file's imports: local name → target file id.
    pub(super) user_imports: HashMap<String, u32>,
}

/// The kind of function a [`FnInfo`] describes, which decides how it is emitted
/// and dispatched.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FnKind {
    /// A direct top-level `such name:` — a static `f_`/`b_` pair, never captures.
    TopLevel,
    /// A nested `such name:` — a `c_`/`cb_` pair that may capture enclosing cells.
    Closure,
    /// An object method — an `mf_`/`mb_` pair, no first-class value, no captures.
    Method,
}

/// The capture/cell analysis for one function definition, keyed by `(file_id,
/// span)` (a span alone is not unique across files).
pub(super) struct FnInfo {
    /// The file this function is defined in — selects name-mangling and scope.
    pub(super) file_id: u32,
    /// The dispatcher id, for functions that can become first-class values
    /// (top-level functions and closures). `None` for methods.
    pub(super) fn_id: Option<u32>,
    pub(super) name: String,
    /// Declared parameters (for a method, `self` is prepended).
    pub(super) params: Vec<String>,
    pub(super) body: Vec<Stmt>,
    /// Names captured from the enclosing scope, in a fixed (sorted) order — the
    /// leading cell parameters this function receives.
    pub(super) captures: Vec<String>,
    /// Every name that is a `Cell` in this function's own frame: its captures,
    /// its nested-function names, and its locals/params a nested closure captures.
    pub(super) cell_names: HashSet<String>,
    pub(super) kind: FnKind,
}

/// A cloned snapshot of a [`FnInfo`]'s emission-relevant fields, so a body can be
/// emitted while `emit` is mutated without holding a borrow into the analysis.
pub(super) struct FnInfoView {
    pub(super) name: String,
    pub(super) params: Vec<String>,
    pub(super) body: Vec<Stmt>,
    pub(super) captures: Vec<String>,
    pub(super) cell_names: HashSet<String>,
}

/// One arm of the generated `call_function` dispatcher, indexed by `fn_id`.
pub(super) enum ArmSpec {
    /// A top-level function: call its recursion-guarded wrapper (mangled by the
    /// defining file's id).
    TopFunc {
        file_id: u32,
        name: String,
        arity: usize,
    },
    /// A closure: call its `c_` wrapper, threading the captured cells first.
    Closure {
        name: String,
        id: u32,
        arity: usize,
        captures: usize,
    },
    /// A builtin used as a value: call the runtime builtin directly.
    Builtin { name: &'static str },
    /// A stdlib module function used as a value: call its runtime function.
    Module {
        name: String,
        runtime_fn: &'static str,
        arity: usize,
    },
}

/// The whole-script function analysis: per-definition capture info, the
/// dispatcher registry, and the name→id lookups for value construction.
pub(super) struct Analysis {
    pub(super) fn_info: HashMap<(u32, Span), FnInfo>,
    pub(super) registry: Vec<ArmSpec>,
    pub(super) top_func_ids: HashMap<(u32, String), u32>,
    pub(super) builtin_ids: HashMap<&'static str, u32>,
    pub(super) module_fn_ids: HashMap<(String, String), u32>,
}

/// Every bound name in one scope — `such`/`so` declarations, `for` loop
/// variables, `oh no` error names, and nested function definitions — in
/// first-seen order, each once. These become the scope's `Env` fields or hoisted
/// locals. A nested function's own body is not descended into: its inner names
/// belong to its own scope.
pub(crate) fn hoisted_names(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    collect_hoisted(stmts, &mut names);
    names
}

/// The top-level hoisted names for `Env` fields: like [`hoisted_names`] but a
/// direct top-level `such name:` / `many Name:` is a static definition, not an
/// `Env` field, so those direct definitions are skipped (a function nested inside
/// a top-level `if`/`for` block is still a closure and does get a field).
pub(crate) fn toplevel_hoisted(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in stmts {
        if matches!(stmt, Stmt::FuncDef { .. } | Stmt::ObjDef { .. }) {
            continue;
        }
        collect_hoisted(std::slice::from_ref(stmt), &mut names);
    }
    names
}

pub(super) fn collect_hoisted(stmts: &[Stmt], names: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Decl { name, .. } | Stmt::ConstDecl { name, .. } => push_unique(names, name),
            // A nested function binds its name in this scope; its body is its own.
            Stmt::FuncDef { name, .. } => push_unique(names, name),
            Stmt::For { var, body, .. } => {
                push_unique(names, var);
                collect_hoisted(body, names);
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (_, body) in branches {
                    collect_hoisted(body, names);
                }
                if let Some(body) = else_body {
                    collect_hoisted(body, names);
                }
            }
            Stmt::While { body, .. } => collect_hoisted(body, names),
            Stmt::Try {
                body,
                err_name,
                handler,
                ..
            } => {
                push_unique(names, err_name);
                collect_hoisted(body, names);
                collect_hoisted(handler, names);
            }
            _ => {}
        }
    }
}

pub(super) fn push_unique(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
    }
}

/// Every identifier referenced anywhere in an expression (attribute names are
/// dynamic, not variables, so they are not collected).
pub(super) fn expr_idents(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Ident { name, .. } => {
            out.insert(name.clone());
        }
        Expr::List { items, .. } => {
            for item in items {
                expr_idents(item, out);
            }
        }
        Expr::Dict { entries, .. } => {
            for (key, value) in entries {
                expr_idents(key, out);
                expr_idents(value, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_idents(lhs, out);
            expr_idents(rhs, out);
        }
        Expr::Unary { operand, .. } => expr_idents(operand, out),
        Expr::Call { callee, args, .. } => {
            expr_idents(callee, out);
            for arg in args {
                expr_idents(arg, out);
            }
        }
        Expr::Index { obj, index, .. } => {
            expr_idents(obj, out);
            expr_idents(index, out);
        }
        Expr::Attr { obj, .. } => expr_idents(obj, out),
        Expr::StrInterp { parts, .. } => {
            for part in parts {
                if let InterpPart::Expr(hole) = part {
                    expr_idents(hole, out);
                }
            }
        }
        _ => {}
    }
}

/// Names referenced in a body, plus the free names of every nested function it
/// contains (which the enclosing scope must supply). Does not descend into a
/// nested function's own body — that is folded in through its free set.
pub(super) fn collect_used(stmts: &[Stmt], used: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Decl { expr, .. }
            | Stmt::ConstDecl { expr, .. }
            | Stmt::Bark { expr, .. }
            | Stmt::Bonk { expr, .. }
            | Stmt::ExprStmt { expr } => expr_idents(expr, used),
            Stmt::Assign { target, expr, .. } => {
                expr_idents(target, used);
                expr_idents(expr, used);
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (cond, body) in branches {
                    expr_idents(cond, used);
                    collect_used(body, used);
                }
                if let Some(body) = else_body {
                    collect_used(body, used);
                }
            }
            Stmt::For { iter, body, .. } => {
                expr_idents(iter, used);
                collect_used(body, used);
            }
            Stmt::While { cond, body, .. } => {
                expr_idents(cond, used);
                collect_used(body, used);
            }
            Stmt::Try { body, handler, .. } => {
                collect_used(body, used);
                collect_used(handler, used);
            }
            Stmt::Return {
                expr: Some(expr), ..
            } => expr_idents(expr, used),
            Stmt::FuncDef { params, body, .. } => {
                for name in free_names(params, body) {
                    used.insert(name);
                }
            }
            _ => {}
        }
    }
}

/// The names a function body references but does not bind — the names it needs
/// from an enclosing scope (or that resolve to globals/builtins).
pub(super) fn free_names(params: &[String], body: &[Stmt]) -> HashSet<String> {
    let mut bound: HashSet<String> = params.iter().cloned().collect();
    for name in hoisted_names(body) {
        bound.insert(name);
    }
    let mut used = HashSet::new();
    collect_used(body, &mut used);
    used.retain(|name| !bound.contains(name));
    used
}

/// The nested functions defined directly in this scope — crossing `if`/`for`/
/// `while`/`pls` blocks (names leak, Python-style) but never another function's
/// body. Returns each `(name, params, body, span)`.
pub(super) fn child_funcdefs(stmts: &[Stmt]) -> Vec<(&str, &[String], &[Stmt], Span)> {
    let mut out = Vec::new();
    collect_child_funcdefs(stmts, &mut out);
    out
}

pub(super) fn collect_child_funcdefs<'a>(
    stmts: &'a [Stmt],
    out: &mut Vec<(&'a str, &'a [String], &'a [Stmt], Span)>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::FuncDef {
                name,
                params,
                body,
                span,
            } => out.push((name, params, body, *span)),
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (_, body) in branches {
                    collect_child_funcdefs(body, out);
                }
                if let Some(body) = else_body {
                    collect_child_funcdefs(body, out);
                }
            }
            Stmt::For { body, .. } | Stmt::While { body, .. } => collect_child_funcdefs(body, out),
            Stmt::Try { body, handler, .. } => {
                collect_child_funcdefs(body, out);
                collect_child_funcdefs(handler, out);
            }
            _ => {}
        }
    }
}

/// The subset of a scope's own bound names that must be shared cells: every
/// nested-function name, plus any local or parameter a nested closure captures.
pub(super) fn celled_locals(params: &[String], body: &[Stmt]) -> HashSet<String> {
    let mut candidates: HashSet<String> = params.iter().cloned().collect();
    for name in hoisted_names(body) {
        candidates.insert(name);
    }
    let mut child_free: HashSet<String> = HashSet::new();
    for (_, child_params, child_body, _) in child_funcdefs(body) {
        for name in free_names(child_params, child_body) {
            child_free.insert(name);
        }
    }
    let funcnames: HashSet<&str> = child_funcdefs(body)
        .iter()
        .map(|(name, _, _, _)| *name)
        .collect();
    candidates
        .into_iter()
        .filter(|name| funcnames.contains(name.as_str()) || child_free.contains(name))
        .collect()
}

/// Gather one file's top-level names and resolved imports into a [`FileTable`].
pub(super) fn file_table(file: &ProgramFile) -> FileTable {
    let mut funcs = HashMap::new();
    let mut consts = HashSet::new();
    let mut func_members = Vec::new();
    let mut const_members = Vec::new();
    for stmt in &file.script.stmts {
        match stmt {
            Stmt::FuncDef { name, params, .. } => {
                funcs.insert(name.clone(), params.clone());
                func_members.push(name.clone());
            }
            Stmt::ConstDecl { name, .. } => {
                consts.insert(name.clone());
                const_members.push(name.clone());
            }
            _ => {}
        }
    }
    // Functions first, then constants — the order module member hints show.
    func_members.extend(const_members);
    FileTable {
        funcs,
        consts,
        members: func_members,
        stdlib_imports: file.stdlib_imports.iter().cloned().collect(),
        user_imports: file.user_imports.iter().cloned().collect(),
    }
}

/// State threaded through the recursive capture analysis.
struct Analyzer {
    fn_info: HashMap<(u32, Span), FnInfo>,
    registry: Vec<ArmSpec>,
    top_func_ids: HashMap<(u32, String), u32>,
    next_id: u32,
}

impl Analyzer {
    /// Analyze one function definition in file `file_id`: record its capture
    /// info, assign its dispatcher id, and recurse into its nested functions.
    /// `enclosing_cells` are the cell names available in the enclosing frame.
    #[allow(clippy::too_many_arguments)]
    fn analyze(
        &mut self,
        file_id: u32,
        name: &str,
        params: &[String],
        body: &[Stmt],
        span: Span,
        kind: FnKind,
        enclosing_cells: &HashSet<String>,
    ) {
        let free = free_names(params, body);
        let mut captures: Vec<String> = free
            .iter()
            .filter(|n| enclosing_cells.contains(*n))
            .cloned()
            .collect();
        captures.sort();

        let mut cell_names = celled_locals(params, body);
        cell_names.extend(captures.iter().cloned());

        let fn_id = match kind {
            FnKind::Method => None,
            FnKind::TopLevel => {
                let id = self.next_id;
                self.next_id += 1;
                self.registry.push(ArmSpec::TopFunc {
                    file_id,
                    name: name.to_string(),
                    arity: params.len(),
                });
                self.top_func_ids.insert((file_id, name.to_string()), id);
                Some(id)
            }
            FnKind::Closure => {
                let id = self.next_id;
                self.next_id += 1;
                self.registry.push(ArmSpec::Closure {
                    name: name.to_string(),
                    id,
                    arity: params.len(),
                    captures: captures.len(),
                });
                Some(id)
            }
        };

        self.fn_info.insert(
            (file_id, span),
            FnInfo {
                file_id,
                fn_id,
                name: name.to_string(),
                params: params.to_vec(),
                body: body.to_vec(),
                captures,
                cell_names: cell_names.clone(),
                kind,
            },
        );

        for (child_name, child_params, child_body, child_span) in child_funcdefs(body) {
            self.analyze(
                file_id,
                child_name,
                child_params,
                child_body,
                child_span,
                FnKind::Closure,
                &cell_names,
            );
        }
    }

    /// Analyze every function in one file: its top-level functions, its object
    /// methods, and the closures nested directly in its top-level blocks.
    fn analyze_file(&mut self, file: &ProgramFile) {
        let file_id = file.file_id;
        let empty = HashSet::new();

        // Direct top-level functions: static, never capture.
        for stmt in &file.script.stmts {
            if let Stmt::FuncDef {
                name,
                params,
                body,
                span,
            } = stmt
            {
                self.analyze(file_id, name, params, body, *span, FnKind::TopLevel, &empty);
            }
        }
        // Object methods: `self` is a bound local; a nested closure may capture it.
        for stmt in &file.script.stmts {
            if let Stmt::ObjDef { methods, .. } = stmt {
                for method in methods {
                    if let Stmt::FuncDef {
                        name,
                        params,
                        body,
                        span,
                    } = method
                    {
                        let mut with_self = Vec::with_capacity(params.len() + 1);
                        with_self.push("self".to_string());
                        with_self.extend(params.iter().cloned());
                        self.analyze(
                            file_id,
                            name,
                            &with_self,
                            body,
                            *span,
                            FnKind::Method,
                            &empty,
                        );
                    }
                }
            }
        }
        // Functions nested inside a top-level block: closures whose enclosing
        // scope is `run`, which holds no cells — so they capture nothing.
        for stmt in &file.script.stmts {
            if matches!(
                stmt,
                Stmt::If { .. } | Stmt::For { .. } | Stmt::While { .. } | Stmt::Try { .. }
            ) {
                for (name, params, body, span) in child_funcdefs(std::slice::from_ref(stmt)) {
                    self.analyze(file_id, name, params, body, span, FnKind::Closure, &empty);
                }
            }
        }
    }
}

/// Walk every file in the program and build the function analysis: capture info
/// per definition, the dispatcher registry, and the name→id lookups. The entry
/// (file 0) is analyzed first so its ids are unchanged from the single-file case.
pub(super) fn analyze_program(program: &Program) -> Analysis {
    let mut analyzer = Analyzer {
        fn_info: HashMap::new(),
        registry: Vec::new(),
        top_func_ids: HashMap::new(),
        next_id: 0,
    };

    for file in &program.files {
        analyzer.analyze_file(file);
    }

    // Builtins as first-class values, then stdlib module functions. Their ids
    // follow every user function so user function ids stay stable.
    let mut builtin_ids = HashMap::new();
    for builtin in BUILTINS {
        let id = analyzer.next_id;
        analyzer.next_id += 1;
        analyzer.registry.push(ArmSpec::Builtin { name: builtin });
        builtin_ids.insert(*builtin, id);
    }
    // A stdlib module can be imported by several files; its runtime functions
    // need one arm each, keyed by the canonical module name.
    let mut module_fn_ids = HashMap::new();
    for file in &program.files {
        for (module_name, module) in &file.stdlib_imports {
            if module_fn_ids
                .keys()
                .any(|(m, _): &(String, String)| m == module_name)
            {
                continue;
            }
            for func in module.funcs {
                let id = analyzer.next_id;
                analyzer.next_id += 1;
                analyzer.registry.push(ArmSpec::Module {
                    name: format!("{module_name}.{}", func.name),
                    runtime_fn: func.runtime_fn,
                    arity: func.arity,
                });
                module_fn_ids.insert((module_name.clone(), func.name.to_string()), id);
            }
        }
    }

    Analysis {
        fn_info: analyzer.fn_info,
        registry: analyzer.registry,
        top_func_ids: analyzer.top_func_ids,
        builtin_ids,
        module_fn_ids,
    }
}
