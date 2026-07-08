#![allow(clippy::result_large_err)]

mod ast;
mod check;
mod codegen;
mod diagnostics;
mod keywords;
mod lexer;
mod parser;
mod token;

pub use ast::{dump, Script};
pub use diagnostics::Diagnostic;

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
    codegen::generate(path, source, script)
}
