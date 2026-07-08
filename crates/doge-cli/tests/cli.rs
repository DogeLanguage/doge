//! End-to-end tests for the `doge` binary: they spawn the real compiled
//! executable (via `CARGO_BIN_EXE_doge`) the way a user would.

use std::path::PathBuf;
use std::process::Command;

fn doge() -> Command {
    Command::new(env!("CARGO_BIN_EXE_doge"))
}

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("doge-compiler")
        .join("tests")
        .join("fixtures")
}

#[test]
fn check_on_a_good_program_dumps_the_ast() {
    let hello = examples_dir().join("hello.doge");
    let output = doge()
        .arg("check")
        .arg(&hello)
        .output()
        .expect("the doge binary should run");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert!(
        stdout.starts_with("Script"),
        "dump should start with Script, got:\n{stdout}"
    );
}

#[test]
fn check_on_a_bad_program_prints_a_diagnostic_to_stderr() {
    let bad = fixtures_dir().join("missing_wow.doge");
    let output = doge()
        .arg("check")
        .arg(&bad)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "expected exit 1");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very"),
        "diagnostic should be doge-flavored, got:\n{stderr}"
    );
    assert!(output.stdout.is_empty(), "no AST dump on failure");
}

#[test]
fn no_arguments_prints_usage_and_exits_two() {
    let output = doge().output().expect("the doge binary should run");
    assert_eq!(output.status.code(), Some(2), "expected exit 2");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("such usage"),
        "should print usage, got:\n{stderr}"
    );
}
