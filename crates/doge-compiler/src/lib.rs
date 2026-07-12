#![allow(clippy::result_large_err)]

mod ast;
mod builtins;
mod check;
mod codegen;
mod complete;
mod diagnostics;
mod fmt;
mod keywords;
mod lexer;
mod manifest;
mod modules;
mod parser;
mod project;
mod stdlib;
mod token;

pub use ast::{
    celled_locals, child_funcdefs, dump, free_names, hoisted_names, BinOp, Expr, InterpPart, Param,
    Params, Script, Stmt, UnOp,
};
pub use builtins::{builtin, is_builtin, BuiltinFn, BuiltinShape, BUILTINS};
pub use check::{check_snippet, ClassInfo, SessionScope};
pub use complete::{complete, Completion, CompletionKind};
pub use diagnostics::Diagnostic;
pub use manifest::{Dependency, DependencySource, GitRev, Manifest, MANIFEST_NAME};
pub use modules::{
    load_program, load_program_with_deps, single_file_program, Program, ProgramFile,
};
pub use parser::{parse_repl, ReplParse};
pub use project::{discover_root, read_manifest, resolve_project, DependencyMap};
pub use stdlib::{module as stdlib_module, Module, ModuleFn, MODULES};
pub use token::Span;

/// Lex and parse `source` (named `path` for diagnostics) into a [`Script`]
pub fn parse(path: &str, source: &str) -> Result<Script, Diagnostic> {
    parser::parse(path, source)
}

/// Run the semantic checks over an already-parsed [`Script`].
pub fn check(path: &str, source: &str, script: &Script) -> Result<(), Diagnostic> {
    check::check(path, source, script)
}

/// Format `source` (named `path` for diagnostics) to canonical Doge style, or a
/// diagnostic if it does not parse. Comments are preserved; the token stream is
/// never changed.
pub fn format(path: &str, source: &str) -> Result<String, Diagnostic> {
    fmt::format(path, source)
}

/// Generate a complete Rust source file from a checked [`Script`], or a
/// diagnostic for a construct it cannot compile (a class used as a value).
pub fn generate(path: &str, source: &str, script: &Script) -> Result<String, Diagnostic> {
    let program = modules::single_file_program(path, source, script.clone())?;
    codegen::generate_program(&program)
}

/// Load the entry script and every `.doge` module it transitively imports.
pub fn load(entry_path: &str, entry_source: &str) -> Result<Program, Diagnostic> {
    modules::load_program(entry_path, entry_source)
}

/// Run the semantic checks over every file in a loaded [`Program`].
pub fn check_program(program: &Program) -> Result<(), Diagnostic> {
    check::check_program(program)
}

/// Generate one Rust source file wiring together every file in a [`Program`].
pub fn generate_program(program: &Program) -> Result<String, Diagnostic> {
    codegen::generate_program(program)
}
