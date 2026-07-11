#![allow(clippy::result_large_err)]

mod ast;
mod builtins;
mod check;
mod codegen;
mod diagnostics;
mod keywords;
mod lexer;
mod modules;
mod parser;
mod stdlib;
mod token;

pub use ast::{dump, Script};
pub use diagnostics::Diagnostic;
pub use modules::{Program, ProgramFile};

/// Lex and parse `source` (named `path` for diagnostics) into a [`Script`]
pub fn parse(path: &str, source: &str) -> Result<Script, Diagnostic> {
    parser::parse(path, source)
}

/// Run the semantic checks over an already-parsed [`Script`].
pub fn check(path: &str, source: &str, script: &Script) -> Result<(), Diagnostic> {
    check::check(path, source, script)
}

/// Generate a complete Rust source file from a checked [`Script`], or a
/// diagnostic pointing at the first feature that only runs in a later milestone.
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
