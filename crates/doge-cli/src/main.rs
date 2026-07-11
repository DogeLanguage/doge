//! The `doge` command-line tool. It exposes three subcommands: `bark` (compile
//! and run), `build` (compile to a standalone binary), and `check` (parse +
//! check → AST dump).

mod build;
mod cache;

use std::path::Path;
use std::process::ExitCode;

/// Exit codes: success, a Doge-level failure (bad program, unreadable file, or a
/// build problem), and a usage error.
const EXIT_OK: u8 = 0;
const EXIT_FAILURE: u8 = 1;
const EXIT_USAGE: u8 = 2;

const USAGE: &str = "such usage: doge <bark|build|check> <script.doge>";
const MISSING_FILE_HEADLINE: &str = "very missing. much file.";
/// Fallback binary name when a script path has no usable stem (e.g. `.doge`).
const DEFAULT_BINARY_STEM: &str = "doge_program";

fn main() -> ExitCode {
    // Skip argv[0] (the program name).
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, path] if cmd == "bark" => run_bark(path),
        [cmd, path] if cmd == "build" => run_build(path),
        [cmd, path] if cmd == "check" => run_check(path),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// `doge bark <path>`: compile the script (using the cache) and run it,
/// propagating the script's own exit code.
fn run_bark(path: &str) -> ExitCode {
    let (source, generated) = match compile_to_rust(path) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    let binary = match build::ensure_binary(&source, &generated) {
        Ok(binary) => binary,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    match build::spawn(&binary) {
        Ok(code) => ExitCode::from(code as u8),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

/// `doge build <path>`: compile the script (using the cache) and drop a
/// standalone binary at `./<script-stem>` in the current directory.
fn run_build(path: &str) -> ExitCode {
    let (source, generated) = match compile_to_rust(path) {
        Ok(pair) => pair,
        Err(code) => return code,
    };
    let binary = match build::ensure_binary(&source, &generated) {
        Ok(binary) => binary,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_FAILURE);
        }
    };
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(DEFAULT_BINARY_STEM);
    match build::copy_to_cwd(&binary, stem) {
        Ok(()) => {
            println!("such binary: ./{stem}");
            ExitCode::from(EXIT_OK)
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

/// `doge check <path>`: load and check the program (the entry and every module
/// it imports), printing the entry's AST dump on success or one doge-flavored
/// diagnostic on failure.
fn run_check(path: &str) -> ExitCode {
    let source = match read_source(path) {
        Ok(source) => source,
        Err(code) => return code,
    };

    let program = match doge_compiler::load(path, &source) {
        Ok(program) => program,
        Err(diag) => {
            eprint!("{}", diag.render());
            return ExitCode::from(EXIT_FAILURE);
        }
    };

    if let Err(diag) = doge_compiler::check_program(&program) {
        eprint!("{}", diag.render());
        return ExitCode::from(EXIT_FAILURE);
    }

    print!("{}", doge_compiler::dump(&program.files[0].script));
    ExitCode::from(EXIT_OK)
}

/// Load, check, and generate Rust from a program — the shared front half of
/// `bark` and `build`. Returns the cache-key source (a blob covering every
/// imported file, so a change to any of them rebuilds) and the generated Rust.
/// On any failure it prints the diagnostic and returns the exit code.
fn compile_to_rust(path: &str) -> Result<(String, String), ExitCode> {
    let source = read_source(path)?;
    let program = match doge_compiler::load(path, &source) {
        Ok(program) => program,
        Err(diag) => {
            eprint!("{}", diag.render());
            return Err(ExitCode::from(EXIT_FAILURE));
        }
    };
    if let Err(diag) = doge_compiler::check_program(&program) {
        eprint!("{}", diag.render());
        return Err(ExitCode::from(EXIT_FAILURE));
    }
    match doge_compiler::generate_program(&program) {
        Ok(generated) => Ok((cache_source(&program), generated)),
        Err(diag) => {
            eprint!("{}", diag.render());
            Err(ExitCode::from(EXIT_FAILURE))
        }
    }
}

/// The cache-key input for a whole program: every file's path and source, so
/// editing any imported module produces a different key (and a fresh build).
fn cache_source(program: &doge_compiler::Program) -> String {
    let mut blob = String::new();
    for file in &program.files {
        blob.push_str(&file.path);
        blob.push('\0');
        blob.push_str(&file.source);
        blob.push('\0');
    }
    blob
}

/// Read a script file, reporting a missing or unreadable file in plain words —
/// never a raw Rust IO error dump (Hard Rule 11).
fn read_source(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|err| {
        eprintln!("{MISSING_FILE_HEADLINE}\n\n  doge cannot read {path}: {err}");
        ExitCode::from(EXIT_FAILURE)
    })
}
