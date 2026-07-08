//! The `doge` command-line tool. In M2 it exposes exactly one subcommand,
//! `check`, matching the roadmap's "AST dump via `doge check`" (DESIGN §8). The
//! `bark` (run) and `build` subcommands arrive with codegen in M3 and are
//! deliberately not stubbed here.
//!
//! Argument parsing is hand-rolled to keep dependencies at zero (CLAUDE.md Hard
//! Rule 10). Nothing here panics: every path returns an [`ExitCode`].

use std::process::ExitCode;

/// Exit codes: success, a Doge-level failure (bad program or unreadable file),
/// and a usage error.
const EXIT_OK: u8 = 0;
const EXIT_FAILURE: u8 = 1;
const EXIT_USAGE: u8 = 2;

const USAGE: &str = "such usage: doge check <script.doge>";

fn main() -> ExitCode {
    // Skip argv[0] (the program name).
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, path] if cmd == "check" => run_check(path),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// `doge check <path>`: parse and check the script, printing its AST dump on
/// success or one doge-flavored diagnostic on failure.
fn run_check(path: &str) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            // A missing or unreadable file is a Doge-level failure, reported in
            // plain words — never a raw Rust IO error dump (Hard Rule 11).
            eprintln!("very missing. much file.\n\n  doge cannot read {path}: {err}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };

    let script = match doge_compiler::parse(path, &source) {
        Ok(script) => script,
        Err(diag) => {
            eprint!("{}", diag.render());
            return ExitCode::from(EXIT_FAILURE);
        }
    };

    if let Err(diag) = doge_compiler::check(path, &source, &script) {
        eprint!("{}", diag.render());
        return ExitCode::from(EXIT_FAILURE);
    }

    print!("{}", doge_compiler::dump(&script));
    ExitCode::from(EXIT_OK)
}
