//! The interpreter parity suite: every `examples/*.doge` that has a `.out` is run
//! through the tree-walking interpreter (the `DOGE_INTERP` harness on `doge bark`)
//! and its stdout must match the committed `.out` byte for byte — the same output
//! the compiled binary produces. This is the guard that the two execution engines
//! never drift apart.

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

/// A scratch working directory for the examples, so one that touches files
/// (`io.doge`) does so on a relative path here instead of the source tree. Named
/// per engine so the compiled and interpreted suites never collide on a file.
fn run_cwd() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_TARGET_TMPDIR"), "/interp-cwd"))
}

#[test]
fn examples_run_identically_under_the_interpreter() {
    let dir = examples_dir();
    let cwd = run_cwd();
    std::fs::create_dir_all(&cwd).expect("scratch cwd should be creatable");
    let mut ran = 0;
    for entry in std::fs::read_dir(&dir).expect("examples directory should exist") {
        let doge = entry.expect("readable dir entry").path();
        if doge.extension().and_then(|e| e.to_str()) != Some("doge") {
            continue;
        }
        let out = doge.with_extension("out");
        if !out.exists() {
            continue;
        }
        ran += 1;

        let expected = std::fs::read_to_string(&out).expect("readable .out file");
        let output = Command::new(env!("CARGO_BIN_EXE_doge"))
            .arg("bark")
            .arg(&doge)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .env("DOGE_INTERP", "1")
            .output()
            .expect("the doge binary should run");

        let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{} should run cleanly under the interpreter, exit={:?}\nstderr:\n{stderr}",
            doge.display(),
            output.status.code(),
        );
        assert_eq!(
            stdout,
            expected,
            "{} interpreted stdout should match its .out",
            doge.display()
        );
    }
    assert!(ran > 0, "no runnable examples with .out files were found");
}

/// Project examples must also run identically under the interpreter, so the
/// dependency-resolution path is exercised by both engines.
#[test]
fn project_examples_run_identically_under_the_interpreter() {
    let dir = examples_dir();
    let mut ran = 0;
    for entry in std::fs::read_dir(&dir).expect("examples directory should exist") {
        let project = entry.expect("readable dir entry").path();
        if !project.is_dir() || !project.join("doge.toml").is_file() {
            continue;
        }
        let out = project.join("expected.out");
        if !out.exists() {
            continue;
        }
        ran += 1;

        let expected = std::fs::read_to_string(&out).expect("readable expected.out file");
        let output = Command::new(env!("CARGO_BIN_EXE_doge"))
            .arg("bark")
            .current_dir(&project)
            .stdin(Stdio::null())
            .env("DOGE_INTERP", "1")
            .output()
            .expect("the doge binary should run");

        let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{} should run cleanly under the interpreter, exit={:?}\nstderr:\n{stderr}",
            project.display(),
            output.status.code(),
        );
        assert_eq!(
            stdout,
            expected,
            "{} interpreted stdout should match its expected.out",
            project.display()
        );
    }
    assert!(ran > 0, "no runnable project examples were found");
}
