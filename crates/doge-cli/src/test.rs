//! `doge test`: discover and run Doge test functions, reporting aggregate pass/fail.
//! A test is a top-level, zero-argument function whose name starts with `test`,
//! written with `amaze` assertions. `doge test <file.doge>` runs one file's tests;
//! `doge test <dir>` runs every `test_*.doge` beneath the directory (recursive).
//! Tests run on the tree-walking interpreter, so each test's error is isolated and
//! reported with its own location — no rustc build is involved.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use doge_compiler::Stmt;

use crate::{EXIT_FAILURE, EXIT_OK, MISSING_FILE_HEADLINE};

/// A function is a test when its name starts with this and it takes no arguments.
const TEST_PREFIX: &str = "test";
/// In a directory, only files whose name starts with this and ends in `.doge` are
/// searched for tests.
const TEST_FILE_PREFIX: &str = "test_";
const DOGE_EXT: &str = ".doge";

/// Every test passed.
const PASS_HEADLINE: &str = "much wow. very pass.";
/// At least one test (or a whole file) failed.
const FAIL_HEADLINE: &str = "very fail. much sad.";
/// No test functions were discovered anywhere.
const EMPTY_HEADLINE: &str = "very empty. much untested.";

/// Running totals across every file in a run. A file that fails to read, load,
/// check, or integrate counts as one `failed` (it discovered nothing), so a broken
/// suite never masquerades as an empty one.
#[derive(Default)]
struct Totals {
    discovered: usize,
    passed: usize,
    failed: usize,
}

/// `doge test <path>`: run the tests in a file, or every `test_*.doge` under a
/// directory. Exits `0` only when at least one test ran and all of them passed.
pub(crate) fn run_test(path: &str) -> ExitCode {
    let files = match collect_files(path) {
        Ok(files) => files,
        Err(code) => return code,
    };

    let mut totals = Totals::default();
    for file in &files {
        run_file(file, &mut totals);
    }

    if totals.discovered == 0 && totals.failed == 0 {
        eprintln!(
            "{EMPTY_HEADLINE}\n\n  no tests found — a test is a top-level function that takes no arguments and whose name starts with `{TEST_PREFIX}`"
        );
        return ExitCode::from(EXIT_FAILURE);
    }

    println!();
    if totals.failed == 0 {
        println!("{PASS_HEADLINE} {} passed.", totals.passed);
        ExitCode::from(EXIT_OK)
    } else {
        println!(
            "{FAIL_HEADLINE} {} passed, {} failed.",
            totals.passed, totals.failed
        );
        ExitCode::from(EXIT_FAILURE)
    }
}

/// Load, check, and run one test file, folding its outcome into `totals`. Any
/// failure to get the file running is reported in doge-flavored form and counted as
/// a single failure so the overall run turns red.
fn run_file(path: &Path, totals: &mut Totals) {
    let display = path.display().to_string();

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("{MISSING_FILE_HEADLINE}\n\n  doge cannot read {display}: {err}");
            totals.failed += 1;
            return;
        }
    };

    let program = match doge_compiler::load(&display, &source) {
        Ok(program) => program,
        Err(diag) => {
            eprint!("{}", diag.render());
            totals.failed += 1;
            return;
        }
    };
    if let Err(diag) = doge_compiler::check_program(&program) {
        eprint!("{}", diag.render());
        totals.failed += 1;
        return;
    }

    let tests: Vec<String> = program.files[0]
        .script
        .stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::FuncDef { name, params, .. }
                if name.starts_with(TEST_PREFIX) && params.is_empty() =>
            {
                Some(name.clone())
            }
            _ => None,
        })
        .collect();
    if tests.is_empty() {
        return;
    }

    let mut interp = doge_interp::Interp::new();
    println!("such test: {display}");
    if let Err(err) = interp.prepare(&program) {
        let (_, line) = interp.error_site();
        println!(
            "  ✗ (setup) — {}: {} ({display}:{line})",
            err.kind.as_str(),
            err.message
        );
        totals.failed += 1;
        return;
    }

    for name in &tests {
        totals.discovered += 1;
        match interp.call_entry_function(name) {
            Ok(_) => {
                println!("  ✓ {name}");
                totals.passed += 1;
            }
            Err(err) => {
                let (_, line) = interp.error_site();
                println!(
                    "  ✗ {name} — {}: {} ({display}:{line})",
                    err.kind.as_str(),
                    err.message
                );
                totals.failed += 1;
            }
        }
    }
}

/// Resolve the path argument into the list of files to run: a single file as-is, or
/// every `test_*.doge` beneath a directory (recursive, sorted for a stable order).
fn collect_files(path: &str) -> Result<Vec<PathBuf>, ExitCode> {
    let root = Path::new(path);
    let meta = std::fs::metadata(root).map_err(|err| {
        eprintln!("{MISSING_FILE_HEADLINE}\n\n  doge cannot read {path}: {err}");
        ExitCode::from(EXIT_FAILURE)
    })?;

    if !meta.is_dir() {
        return Ok(vec![root.to_path_buf()]);
    }

    let mut files = Vec::new();
    collect_dir(root, &mut files)?;
    files.sort();
    Ok(files)
}

/// Walk `dir` recursively, collecting every `test_*.doge` file into `out`.
fn collect_dir(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ExitCode> {
    let entries = std::fs::read_dir(dir).map_err(|err| {
        eprintln!(
            "{MISSING_FILE_HEADLINE}\n\n  doge cannot read {}: {err}",
            dir.display()
        );
        ExitCode::from(EXIT_FAILURE)
    })?;

    for entry in entries {
        let entry = entry.map_err(|err| {
            eprintln!(
                "{MISSING_FILE_HEADLINE}\n\n  doge cannot read {}: {err}",
                dir.display()
            );
            ExitCode::from(EXIT_FAILURE)
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_dir(&path, out)?;
        } else if is_test_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Whether a file's name marks it as a test file for directory discovery.
fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(TEST_FILE_PREFIX) && name.ends_with(DOGE_EXT))
}
