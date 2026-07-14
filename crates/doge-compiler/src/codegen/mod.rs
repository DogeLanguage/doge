//! Doge AST → Rust source. The generated Rust is thin glue: every
//! value operation calls a function in `doge-runtime`, so behaviour lives there,
//! not in these strings.

pub(super) use std::cell::{Cell, RefCell};
pub(super) use std::collections::{HashMap, HashSet};

pub(super) use crate::ast::{
    child_funcdefs, hoisted_names, toplevel_hoisted, BinOp, Expr, InterpPart, Params, Stmt, UnOp,
};
pub(super) use crate::builtins::{BuiltinFn, BuiltinShape};
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
mod program;
mod stmt;
#[cfg(test)]
mod tests;

use analysis::*;
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
            lines: crate::diagnostics::split_source_lines(&f.source),
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

/// The `doge-runtime` function that implements a binary operator, shared by the
/// `Binary` expression and augmented-assignment codegen so the mapping lives in
/// one place. The short-circuit `and`/`or` are emitted inline, never as a call.
pub(super) fn binop_call(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::FloorDiv => "floordiv",
        BinOp::Rem => "rem",
        BinOp::Pow => "pow",
        BinOp::BitAnd => "bitand",
        BinOp::BitOr => "bitor",
        BinOp::BitXor => "bitxor",
        BinOp::Shl => "shl",
        BinOp::Shr => "shr",
        BinOp::Eq => "eq",
        BinOp::NotEq => "ne",
        BinOp::Lt => "lt",
        BinOp::LtEq => "le",
        BinOp::Gt => "gt",
        BinOp::GtEq => "ge",
        BinOp::In => "in_",
        BinOp::NotIn => "not_in",
        BinOp::And | BinOp::Or => {
            unreachable!("compiler bug: and/or are emitted inline, not as a call")
        }
    }
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

/// An empty parameter header, for a class whose `init` is absent (it constructs
/// from zero arguments).
static EMPTY_PARAMS: Params = Params {
    params: Vec::new(),
    vararg: None,
};

/// A top-level `many Name:` object definition. `id` is its program-wide index —
/// the class tag stored on every instance and matched by the dispatcher — assigned
/// across every file so a module's objects and the entry's never collide.
struct Class {
    /// The file this class is defined in — selects name-mangling for its member
    /// calls and scopes name resolution (two files may each define a `Shibe`).
    file_id: u32,
    name: String,
    id: u32,
    /// The class id this one inherits from, when `many Name much Parent:` gave one.
    parent: Option<u32>,
    /// Each method as `(name, params)`; `params` excludes the implicit `self`.
    methods: Vec<(String, Params)>,
}

impl Class {
    /// This class's own `init` parameters, if it defines one directly. Inherited
    /// `init` is resolved through the ancestry (see [`effective_init`]).
    fn own_init(&self) -> Option<&Params> {
        self.methods
            .iter()
            .find(|(name, _)| name == "init")
            .map(|(_, params)| params)
    }
}

/// A class's ancestry, nearest first and starting with the class itself: `[self,
/// parent, grandparent, …]`. A cycle (which the checker rejects) is broken
/// defensively so codegen never loops.
fn ancestry<'a>(classes: &'a [Class], class: &'a Class) -> Vec<&'a Class> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = Some(class);
    while let Some(c) = cur {
        if !seen.insert(c.id) {
            break;
        }
        chain.push(c);
        cur = c
            .parent
            .and_then(|pid| classes.iter().find(|x| x.id == pid));
    }
    chain
}

/// The nearest `init` in `class`'s ancestry: its defining class and that `init`'s
/// parameters. `None` when no class in the chain defines one (construction then
/// takes no arguments).
fn effective_init<'a>(classes: &'a [Class], class: &'a Class) -> Option<(&'a Class, &'a Params)> {
    ancestry(classes, class)
        .into_iter()
        .find_map(|c| c.own_init().map(|params| (c, params)))
}

/// The parameters `class` constructs from: its effective `init`'s, or an empty
/// header when nothing in the chain defines `init`.
fn init_params<'a>(classes: &'a [Class], class: &'a Class) -> &'a Params {
    effective_init(classes, class)
        .map(|(_, params)| params)
        .unwrap_or(&EMPTY_PARAMS)
}

/// Every method callable on an instance of `class`, with the class that defines
/// each (nearest override wins) and its parameters. Drives one dispatcher arm per
/// method: an inherited method dispatches to its ancestor's `mf_` wrapper.
fn effective_methods<'a>(
    classes: &'a [Class],
    class: &'a Class,
) -> Vec<(&'a str, &'a Class, &'a Params)> {
    let mut out: Vec<(&str, &Class, &Params)> = Vec::new();
    for c in ancestry(classes, class) {
        for (name, params) in &c.methods {
            if !out.iter().any(|(seen, _, _)| *seen == name.as_str()) {
                out.push((name.as_str(), c, params));
            }
        }
    }
    out
}

/// The mutable state threaded through one function's (or `run`'s) emission.
struct Emit<'a> {
    /// Every file's top-level names, indexed by `file_id`.
    tables: &'a [FileTable],
    /// The file whose code is currently being emitted — selects name-mangling
    /// and which table resolves bare names, calls, and imports.
    file_id: u32,
    /// Every object definition across the program, in file-then-source order.
    classes: &'a [Class],
    /// The class whose method body is currently being emitted, if any — set while
    /// emitting a method so a `super` call can resolve against its parent.
    current_class: Option<u32>,
    /// The whole-script function analysis: capture info and value ids.
    analysis: &'a Analysis,
    /// Set once any method-call site is compiled, so the dispatcher is emitted
    /// even when a script calls methods but defines no objects of its own.
    uses_method_call: Cell<bool>,
    /// Set once any indirect call is compiled, so the `call_function` dispatcher
    /// is emitted.
    uses_call_function: Cell<bool>,
    /// Set once any bare `obj.name` value read is compiled, so `class_has_method`
    /// (the bound-method gate) is emitted.
    uses_attr_read: Cell<bool>,
    /// Set once a `pack.zoom` call is compiled, so the pup trampoline
    /// (`snapshot_env` + `pup_entry`) is emitted.
    uses_pup_entry: Cell<bool>,
    /// Every `fn_id` turned into a `Value::function` somewhere — the dispatcher
    /// arms it must carry.
    materialized: RefCell<HashSet<u32>>,
    /// Names local to the code being emitted → how they are stored. Empty while
    /// emitting `run`, where every bound name is an `Env` field.
    locals: HashMap<String, Local>,
    /// Nested-function names in scope → their declared parameters, for
    /// compile-time arity checks on direct calls. Reset per callable.
    local_funcs: HashMap<String, Params>,
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

    /// The class named `name` defined in the file currently being emitted, if any.
    /// Resolution is per-file: two files may each define a `Shibe`.
    fn class(&self, name: &str) -> Option<&Class> {
        self.class_in(self.file_id, name)
    }

    /// The class named `name` defined in file `file_id`, if any — used to resolve a
    /// module-qualified constructor `utils.Shibe(…)` against the module's classes.
    fn class_in(&self, file_id: u32, name: &str) -> Option<&Class> {
        self.classes
            .iter()
            .find(|c| c.file_id == file_id && c.name == name)
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
        out.push_str("    set_script_args(std::env::args().skip(1).collect());\n");
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
