//! Whole-program function/capture analysis and scope-name collection.

use std::collections::{HashMap, HashSet};

use crate::ast::{celled_locals, child_funcdefs, free_names, Params, Stmt};
use crate::builtins::BUILTINS;
use crate::modules::{Program, ProgramFile};
use crate::stdlib::Module;
use crate::token::Span;

/// A file's top-level names, gathered once so any file's code can resolve calls,
/// constants, and members into another file. Indexed program-wide by `file_id`.
pub(super) struct FileTable {
    /// Top-level function name → its declared parameters (with defaults/variadic).
    pub(super) funcs: HashMap<String, Params>,
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
    /// The names the body binds: `self` (methods only), then each parameter, then
    /// the variadic — the value parameters the compiled wrapper takes.
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
    /// defining file's id). `params` drives the arm's range arity check and its
    /// default/variadic filling.
    TopFunc {
        file_id: u32,
        name: String,
        params: Params,
    },
    /// A closure: call its `c_` wrapper, threading the captured cells first.
    Closure {
        name: String,
        id: u32,
        params: Params,
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

/// Gather one file's top-level names and resolved imports into a [`FileTable`].
pub(super) fn file_table(file: &ProgramFile) -> FileTable {
    let mut funcs = HashMap::new();
    let mut consts = HashSet::new();
    let mut func_members = Vec::new();
    let mut class_members = Vec::new();
    let mut const_members = Vec::new();
    for stmt in &file.script.stmts {
        match stmt {
            Stmt::FuncDef { name, params, .. } => {
                funcs.insert(name.clone(), params.clone());
                func_members.push(name.clone());
            }
            Stmt::ObjDef { name, .. } => {
                class_members.push(name.clone());
            }
            Stmt::ConstDecl { name, .. } => {
                consts.insert(name.clone());
                const_members.push(name.clone());
            }
            _ => {}
        }
    }
    // Functions, then classes, then constants — the order module member hints show.
    func_members.extend(class_members);
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
        params: &Params,
        body: &[Stmt],
        span: Span,
        kind: FnKind,
        enclosing_cells: &HashSet<String>,
    ) {
        // A method binds `self` ahead of its declared parameters; every kind then
        // binds each parameter plus the variadic (which arrives as a packed List).
        let mut binding = Vec::new();
        if kind == FnKind::Method {
            binding.push("self".to_string());
        }
        binding.extend(params.binding_names());

        let free = free_names(&binding, body);
        let mut captures: Vec<String> = free
            .iter()
            .filter(|n| enclosing_cells.contains(*n))
            .cloned()
            .collect();
        captures.sort();

        let mut cell_names = celled_locals(&binding, body);
        cell_names.extend(captures.iter().cloned());

        let fn_id = match kind {
            FnKind::Method => None,
            FnKind::TopLevel => {
                let id = self.next_id;
                self.next_id += 1;
                self.registry.push(ArmSpec::TopFunc {
                    file_id,
                    name: name.to_string(),
                    params: params.clone(),
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
                    params: params.clone(),
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
                params: binding,
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
                        self.analyze(file_id, name, params, body, *span, FnKind::Method, &empty);
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
        analyzer
            .registry
            .push(ArmSpec::Builtin { name: builtin.name });
        builtin_ids.insert(builtin.name, id);
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
