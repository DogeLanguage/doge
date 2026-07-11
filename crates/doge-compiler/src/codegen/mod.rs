//! Doge AST → Rust source. The generated Rust is thin glue: every
//! value operation calls a function in `doge-runtime`, so behaviour lives there,
//! not in these strings.

pub(super) use std::cell::{Cell, RefCell};
pub(super) use std::collections::{HashMap, HashSet};

pub(super) use crate::ast::{BinOp, Expr, InterpPart, Stmt, UnOp};
pub(super) use crate::check::BUILTINS;
pub(super) use crate::diagnostics::Diagnostic;
pub(super) use crate::modules::Program;
pub(super) use crate::stdlib::Module;
pub(super) use crate::token::Span;

mod analysis;
mod callable;
mod calls;
mod dispatch;
mod expr;
mod names;
mod stmt;
#[cfg(test)]
mod tests;

use analysis::*;
pub(crate) use analysis::{hoisted_names, toplevel_hoisted};
use names::*;

/// Turn a checked [`Program`] into a complete Rust source file, or a diagnostic
/// pointing at the first feature the current milestone cannot run yet. A
/// single-file program keeps the exact output shape it always had; a program
/// with modules threads a per-file source table for cross-file error locations.
pub fn generate_program(program: &Program) -> Result<String, Diagnostic> {
    let files = program
        .files
        .iter()
        .map(|f| FileText {
            path: f.path.clone(),
            lines: f
                .source
                .split('\n')
                .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
                .collect(),
        })
        .collect();
    let codegen = Codegen {
        files,
        multifile: program.files.len() > 1,
        cur: Cell::new(0),
    };
    codegen.program(program)
}

/// The path and source lines of one program file, indexed by `file_id`.
struct FileText {
    path: String,
    lines: Vec<String>,
}

struct Codegen {
    /// One entry per file, indexed by `file_id`. `files[0]` is the entry.
    files: Vec<FileText>,
    /// True when the program has more than one file, so error reporting must
    /// carry the file id at runtime (a single-file program keeps its old shape).
    multifile: bool,
    /// The file whose text `diag`/`self.path()`/`self.line()` currently render
    /// against — set alongside `Emit::file_id` whenever emission switches files.
    cur: Cell<u32>,
}

/// A top-level `many Name:` object definition. `id` is its source-order index —
/// the class tag stored on every instance and matched by the dispatcher.
struct Class {
    name: String,
    id: u32,
    /// Each method as `(name, params)`; `params` excludes the implicit `self`.
    methods: Vec<(String, Vec<String>)>,
}

impl Class {
    /// The parameters of this class's `init`, or an empty slice when it has none
    /// (a class without `init` constructs from zero arguments).
    fn init_params(&self) -> &[String] {
        self.methods
            .iter()
            .find(|(name, _)| name == "init")
            .map(|(_, params)| params.as_slice())
            .unwrap_or(&[])
    }
}

/// The mutable state threaded through one function's (or `run`'s) emission.
struct Emit<'a> {
    /// Every file's top-level names, indexed by `file_id`.
    tables: &'a [FileTable],
    /// The file whose code is currently being emitted — selects name-mangling
    /// and which table resolves bare names, calls, and imports.
    file_id: u32,
    /// Every top-level object definition, in source order (entry only).
    classes: &'a [Class],
    /// The whole-script function analysis: capture info and value ids.
    analysis: &'a Analysis,
    /// Set once any method-call site is compiled, so the dispatcher is emitted
    /// even when a script calls methods but defines no objects of its own.
    uses_method_call: Cell<bool>,
    /// Set once any indirect call is compiled, so the `call_function` dispatcher
    /// is emitted.
    uses_call_function: Cell<bool>,
    /// Every `fn_id` turned into a `Value::function` somewhere — the dispatcher
    /// arms it must carry.
    materialized: RefCell<HashSet<u32>>,
    /// Names local to the code being emitted → how they are stored. Empty while
    /// emitting `run`, where every bound name is an `Env` field.
    locals: HashMap<String, Local>,
    /// Nested-function names in scope → their parameter names, for compile-time
    /// arity checks on direct calls. Reset per callable.
    local_funcs: HashMap<String, Vec<String>>,
    /// Monotonic per-file counter naming `'pN` try labels, `attemptN` binders,
    /// and `'lN` loop labels.
    counter: u32,
    /// Enclosing `pls` labels, innermost last: a fallible call diverts to the
    /// last one instead of `?`-ing out of the function.
    try_stack: Vec<u32>,
    /// Enclosing loop labels, innermost last: `bork`/`continue` target the last.
    loop_stack: Vec<u32>,
}

impl Emit<'_> {
    /// The current file's table.
    fn table(&self) -> &FileTable {
        &self.tables[self.file_id as usize]
    }

    /// The class named `name`, if one is defined.
    fn class(&self, name: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.name == name)
    }

    /// The stdlib module imported as `name` in the current file, if any — but a
    /// local of the same name shadows it (locals always win at a use site).
    fn module(&self, name: &str) -> Option<&'static Module> {
        if self.locals.contains_key(name) {
            None
        } else {
            self.table().stdlib_imports.get(name).copied()
        }
    }

    /// The user module imported as `name` in the current file (its `file_id`), if
    /// any — again shadowed by a local of the same name.
    fn user_module(&self, name: &str) -> Option<u32> {
        if self.locals.contains_key(name) {
            None
        } else {
            self.table().user_imports.get(name).copied()
        }
    }

    /// The dispatcher id for a bare function name used as a value: a top-level
    /// function of the current file, or a builtin. Nested functions are ordinary
    /// cell locals, so they are not handled here.
    fn func_value_id(&self, name: &str) -> Option<u32> {
        self.analysis
            .top_func_ids
            .get(&(self.file_id, name.to_string()))
            .copied()
            .or_else(|| self.analysis.builtin_ids.get(name).copied())
    }
}

/// A language feature the parser accepts but the current milestone cannot run
/// yet, with the exact message and the milestone that lands it.
enum Unsupported {
    ClassAsValue(String),
}

impl Unsupported {
    fn detail(&self) -> (String, &'static str) {
        match self {
            Unsupported::ClassAsValue(name) => (
                format!("{name} is an object definition — objects as values land in M6"),
                "M6",
            ),
        }
    }
}

/// How a name is stored in the code currently being emitted: a plain `Value`
/// binding, or a shared `Cell` (because a nested closure captures it).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Local {
    Plain,
    Cell,
}

impl Codegen {
    /// Move emission to `file_id`: switch the file `diag`/error-reporting renders
    /// against, and the file whose scope/name-mangling `emit` uses.
    fn enter_file(&self, emit: &mut Emit, file_id: u32) {
        self.cur.set(file_id);
        emit.file_id = file_id;
    }

    fn program(&self, program: &Program) -> Result<String, Diagnostic> {
        let tables: Vec<FileTable> = program.files.iter().map(file_table).collect();

        let analysis = analyze_program(program);

        // Objects are entry-only (a module with an object is a check error), so
        // the class list comes from the entry alone; its index is the class id.
        let entry = &program.files[0];
        let mut classes: Vec<Class> = Vec::new();
        for stmt in &entry.script.stmts {
            if let Stmt::ObjDef { name, methods, .. } = stmt {
                let methods = methods
                    .iter()
                    .filter_map(|m| match m {
                        Stmt::FuncDef { name, params, .. } => Some((name.clone(), params.clone())),
                        _ => None,
                    })
                    .collect();
                classes.push(Class {
                    name: name.clone(),
                    id: classes.len() as u32,
                    methods,
                });
            }
        }

        // The `Env` holds the line tracker, the recursion depth, and every file's
        // top-level bound names: the entry's `v_` fields, then each module's `g_`
        // constant fields. A direct top-level function/object is a static
        // definition, not a field.
        let mut env_fields: Vec<String> = toplevel_hoisted(&entry.script.stmts)
            .iter()
            .map(|name| field_name(0, name))
            .collect();
        for file in &program.files[1..] {
            for name in &tables[file.file_id as usize].members {
                if tables[file.file_id as usize].consts.contains(name) {
                    env_fields.push(field_name(file.file_id, name));
                }
            }
        }

        let mut emit = Emit {
            tables: &tables,
            file_id: 0,
            classes: &classes,
            analysis: &analysis,
            uses_method_call: Cell::new(false),
            uses_call_function: Cell::new(false),
            materialized: RefCell::new(HashSet::new()),
            locals: HashMap::new(),
            local_funcs: HashMap::new(),
            counter: 0,
            try_stack: Vec::new(),
            loop_stack: Vec::new(),
        };

        // `run` first initializes every module's constants in dependency order
        // (so a module referencing another's constant sees it ready), then runs
        // the entry's own top-level statements.
        let mut run_body = String::new();
        for &fid in &program.init_order {
            self.enter_file(&mut emit, fid);
            for stmt in &program.files[fid as usize].script.stmts {
                if matches!(stmt, Stmt::ConstDecl { .. }) {
                    self.stmt(stmt, 1, &mut emit, &mut run_body)?;
                }
            }
        }
        self.enter_file(&mut emit, 0);
        for stmt in &entry.script.stmts {
            if matches!(
                stmt,
                Stmt::FuncDef { .. } | Stmt::ObjDef { .. } | Stmt::Import { .. }
            ) {
                continue;
            }
            self.stmt(stmt, 1, &mut emit, &mut run_body)?;
        }

        // Every file's top-level functions, mangled by file id.
        let mut funcs_src = String::new();
        for file in &program.files {
            self.enter_file(&mut emit, file.file_id);
            for stmt in &file.script.stmts {
                if let Stmt::FuncDef { span, .. } = stmt {
                    self.function(*span, &mut emit, &mut funcs_src)?;
                }
            }
        }

        // Each entry object contributes a constructor plus a wrapper/body pair
        // per method; the source is looked up from the ObjDef, keyed by class id.
        self.enter_file(&mut emit, 0);
        for (class, stmt) in classes.iter().zip(
            entry
                .script
                .stmts
                .iter()
                .filter(|s| matches!(s, Stmt::ObjDef { .. })),
        ) {
            let Stmt::ObjDef { methods, .. } = stmt else {
                unreachable!("compiler bug: class list and ObjDef filter disagree")
            };
            self.constructor(class, &mut funcs_src);
            for method in methods {
                if let Stmt::FuncDef { span, .. } = method {
                    self.method(class, *span, &mut emit, &mut funcs_src)?;
                }
            }
        }

        // Every closure — nested functions, at any depth, in any file — is
        // emitted as a `c_`/`cb_` pair. Ordered by id for stable output.
        let mut closures: Vec<&FnInfo> = analysis
            .fn_info
            .values()
            .filter(|info| info.kind == FnKind::Closure)
            .collect();
        closures.sort_by_key(|info| info.fn_id);
        for info in closures {
            self.enter_file(&mut emit, info.file_id);
            self.closure(info, &mut emit, &mut funcs_src)?;
        }

        let dispatcher = self.dispatcher(&classes, emit.uses_method_call.get());
        let fn_dispatcher = self.function_dispatcher(&emit);

        let mut out = String::new();
        out.push_str("#![allow(warnings)]\n");
        out.push_str("use doge_runtime::*;\n\n");
        self.emit_source_tables(&mut out);
        self.emit_env_and_main(&env_fields, &mut out);

        out.push_str("fn run(env: &mut Env) -> DogeResult<()> {\n");
        out.push_str(&run_body);
        out.push_str("    Ok(())\n");
        out.push_str("}\n");

        out.push_str(&funcs_src);
        out.push_str(&dispatcher);
        out.push_str(&fn_dispatcher);
        Ok(out)
    }

    /// The embedded source lines an uncaught error shows without any Rust
    /// backtrace. A single-file program keeps its `LINES` table verbatim; a
    /// multi-file program emits one table per file plus a `FILES` index.
    fn emit_source_tables(&self, out: &mut String) {
        if !self.multifile {
            out.push_str("static LINES: &[&str] = &[\n");
            for line in &self.files[0].lines {
                out.push_str(&format!("    \"{}\",\n", escape_str(line)));
            }
            out.push_str("];\n\n");
            return;
        }
        for (id, file) in self.files.iter().enumerate() {
            out.push_str(&format!("static L{id}: &[&str] = &[\n"));
            for line in &file.lines {
                out.push_str(&format!("    \"{}\",\n", escape_str(line)));
            }
            out.push_str("];\n");
        }
        out.push_str("static FILES: &[(&str, &[&str])] = &[\n");
        for (id, file) in self.files.iter().enumerate() {
            out.push_str(&format!("    (\"{}\", L{id}),\n", escape_str(&file.path)));
        }
        out.push_str("];\n\n");
    }

    /// The `Env` struct, `main`, and the uncaught-error reporter. The single-file
    /// form embeds the one path; the multi-file form carries `cur_file` and looks
    /// the offending file up in `FILES`.
    fn emit_env_and_main(&self, env_fields: &[String], out: &mut String) {
        out.push_str("struct Env {\n");
        out.push_str("    cur_line: u32,\n");
        if self.multifile {
            out.push_str("    cur_file: u32,\n");
        }
        out.push_str("    depth: usize,\n");
        for field in env_fields {
            out.push_str(&format!("    {field}: Value,\n"));
        }
        out.push_str("}\n\n");

        out.push_str("fn main() -> std::process::ExitCode {\n");
        out.push_str("    let mut env = Env {\n");
        out.push_str("        cur_line: 0,\n");
        if self.multifile {
            out.push_str("        cur_file: 0,\n");
        }
        out.push_str("        depth: 0,\n");
        for field in env_fields {
            out.push_str(&format!("        {field}: Value::None,\n"));
        }
        out.push_str("    };\n");
        out.push_str("    match run(&mut env) {\n");
        out.push_str("        Ok(()) => std::process::ExitCode::SUCCESS,\n");
        out.push_str("        Err(e) => {\n");
        if self.multifile {
            out.push_str("            let (f_path, f_lines) = FILES[env.cur_file as usize];\n");
            out.push_str(
                "            let line = f_lines.get((env.cur_line as usize).saturating_sub(1)).copied().unwrap_or(\"\");\n",
            );
            out.push_str(&format!(
                "            eprintln!(\"{}\\n\\n  {{}}:{{}}\\n    {{}}\\n  {{}}\", f_path, env.cur_line, line, e);\n",
                RUNTIME_ERROR_HEADLINE,
            ));
        } else {
            out.push_str(
                "            let line = LINES.get((env.cur_line as usize).saturating_sub(1)).copied().unwrap_or(\"\");\n",
            );
            out.push_str(&format!(
                "            eprintln!(\"{}\\n\\n  {}:{{}}\\n    {{line}}\\n  {{e}}\", env.cur_line);\n",
                RUNTIME_ERROR_HEADLINE,
                escape_str(&self.files[0].path)
            ));
        }
        out.push_str("            std::process::ExitCode::FAILURE\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    /// Look up a definition's capture analysis by its `(file_id, span)` key.
    /// Cloned because the borrow would otherwise conflict with mutating `emit`
    /// while emitting.
    fn fn_info(&self, emit: &Emit, span: Span) -> FnInfoView {
        let info = emit
            .analysis
            .fn_info
            .get(&(emit.file_id, span))
            .expect("compiler bug: definition was not analyzed");
        FnInfoView {
            name: info.name.clone(),
            params: info.params.clone(),
            body: info.body.clone(),
            captures: info.captures.clone(),
            cell_names: info.cell_names.clone(),
        }
    }

    fn unsupported(&self, span: Span, feature: Unsupported) -> Diagnostic {
        let (message, milestone) = feature.detail();
        self.diag(span, message)
            .with_headline(UNSUPPORTED_HEADLINE)
            .with_hint(format!(
                "doge check already understands this script — running it lands in {milestone}"
            ))
    }

    fn diag(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        let file = &self.files[self.cur.get() as usize];
        let source_line = file
            .lines
            .get((span.line as usize).saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        Diagnostic::new(&file.path, span.line, span.col, source_line, message)
    }
}
