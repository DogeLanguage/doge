//! The `doge` command-line tool. Its subcommands: `bark` (compile and run),
//! `build` (compile to a standalone binary), `check` (parse + check → AST dump),
//! `fmt` (format in place, or `--check`), `test` (discover and run `test`-prefixed
//! functions), `lsp` (language server), and `repl` (interactive interpreter, also
//! the default when run with no arguments).

mod build;
mod cache;
mod repl;
mod test;

use std::path::Path;
use std::process::ExitCode;

/// Exit codes: success, a Doge-level failure (bad program, unreadable file, or a
/// build problem), and a usage error.
const EXIT_OK: u8 = 0;
const EXIT_FAILURE: u8 = 1;
const EXIT_USAGE: u8 = 2;

const USAGE: &str = "such usage: doge <bark|build|check|fmt|test|lsp|repl> [script.doge]";
/// Headline when the language server's transport fails — an editor/protocol
/// problem, never a Doge program error.
const LSP_ERROR_HEADLINE: &str = "very server. much broken.";
const MISSING_FILE_HEADLINE: &str = "very missing. much file.";
/// Headline for `doge fmt --check` when a file is not already formatted.
const UNFORMATTED_HEADLINE: &str = "very messy. much unformatted.";
/// The headline for an uncaught runtime error, shared with the compiled program.
const RUNTIME_ERROR_HEADLINE: &str = "very error. much broken.";
/// Fallback binary name when a script path has no usable stem (e.g. `.doge`).
const DEFAULT_BINARY_STEM: &str = "doge_program";
/// When set, `doge bark` runs the script through the tree-walking interpreter
/// instead of compiling it — the harness the examples parity suite drives to
/// prove the two engines agree. Not a user-facing flag.
const INTERP_ENV: &str = "DOGE_INTERP";

fn main() -> ExitCode {
    // Skip argv[0] (the program name).
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, path, rest @ ..] if cmd == "bark" => run_bark(path, rest),
        [cmd, path] if cmd == "build" => run_build(path),
        [cmd, path] if cmd == "check" => run_check(path),
        [cmd, path] if cmd == "fmt" => run_fmt(path, false),
        [cmd, flag, path] if cmd == "fmt" && flag == "--check" => run_fmt(path, true),
        // `doge test <file|dir>` discovers and runs test functions on the interpreter.
        [cmd, path] if cmd == "test" => {
            let path = path.to_string();
            on_big_stack(move || test::run_test(&path))
        }
        // `doge lsp` starts the language server, speaking LSP over stdin/stdout.
        [cmd] if cmd == "lsp" => run_lsp(),
        // `doge repl`, or a bare `doge`, starts the interactive interpreter.
        [cmd] if cmd == "repl" => on_big_stack(repl::run),
        [] => on_big_stack(repl::run),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// `doge bark <path> [args…]`: compile the script (using the cache) and run it,
/// forwarding any trailing arguments to the program and propagating its exit code.
fn run_bark(path: &str, args: &[String]) -> ExitCode {
    if std::env::var_os(INTERP_ENV).is_some() {
        let path = path.to_string();
        let args = args.to_vec();
        return on_big_stack(move || run_interpreted(&path, args));
    }
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
    match build::spawn(&binary, args) {
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

/// `doge fmt <path>`: rewrite the script in canonical style, writing only when
/// it changes. With `--check`, write nothing and exit non-zero if the file is not
/// already formatted (for CI).
fn run_fmt(path: &str, check: bool) -> ExitCode {
    let source = match read_source(path) {
        Ok(source) => source,
        Err(code) => return code,
    };
    let formatted = match doge_compiler::format(path, &source) {
        Ok(formatted) => formatted,
        Err(diag) => {
            eprint!("{}", diag.render());
            return ExitCode::from(EXIT_FAILURE);
        }
    };

    if check {
        if formatted == source {
            return ExitCode::from(EXIT_OK);
        }
        eprintln!("{UNFORMATTED_HEADLINE}\n\n  {path} needs formatting: run doge fmt {path}");
        return ExitCode::from(EXIT_FAILURE);
    }

    if formatted == source {
        return ExitCode::from(EXIT_OK);
    }
    match std::fs::write(path, &formatted) {
        Ok(()) => {
            println!("such format: {path}");
            ExitCode::from(EXIT_OK)
        }
        Err(err) => {
            eprintln!("very disk. much sad.\n\n  doge could not write {path}: {err}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

/// `doge lsp`: run the language server, serving diagnostics and completion to an
/// editor over stdin/stdout. It runs until the client disconnects; a transport
/// failure is reported in doge-flavored form (it is never a user program error).
fn run_lsp() -> ExitCode {
    match doge_lsp::run_stdio() {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(err) => {
            eprintln!("{LSP_ERROR_HEADLINE}\n\n  the doge language server stopped: {err}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

/// Run a script through the tree-walking interpreter (the `DOGE_INTERP` path):
/// load, check, then evaluate the program directly, reporting an uncaught error in
/// the same doge-flavored form the compiled program uses — never raw Rust. The
/// script's arguments are published so `env.args()` sees the same list the
/// compiled program's `main` would.
fn run_interpreted(path: &str, args: Vec<String>) -> ExitCode {
    doge_runtime::set_script_args(args);
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
    let mut interp = doge_interp::Interp::new();
    match interp.run(&program) {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(err) => {
            let (fid, line) = interp.error_site();
            let file = &program.files[fid];
            let src_line = file
                .source
                .lines()
                .nth((line as usize).saturating_sub(1))
                .unwrap_or("");
            eprintln!(
                "{RUNTIME_ERROR_HEADLINE}\n\n  {}:{}\n    {}\n  {}",
                file.path, line, src_line, err
            );
            ExitCode::from(EXIT_FAILURE)
        }
    }
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

/// The tree-walking interpreter recurses on the native stack — one Doge call
/// nests several Rust frames — so a deep (but within-limit) Doge recursion needs
/// far more stack than a thread's default. Run interpreter work on a thread with a
/// generous stack so the catchable recursion limit is what stops runaway recursion,
/// never a stack overflow. Only `Send` results cross back; the `Rc`-based
/// interpreter is created and dropped entirely inside the thread.
fn on_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    const STACK: usize = 256 * 1024 * 1024;
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(f)
        .expect("spawning the interpreter thread")
        .join()
        .expect("the interpreter thread panicked")
}

/// Read a script file, reporting a missing or unreadable file in plain words —
/// never a raw Rust IO error dump (Hard Rule 11).
fn read_source(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|err| {
        eprintln!("{MISSING_FILE_HEADLINE}\n\n  doge cannot read {path}: {err}");
        ExitCode::from(EXIT_FAILURE)
    })
}
