//! The interpreter parity suite: every `examples/*.doge` that has a `.out` is run
//! through the tree-walking interpreter (the `DOGE_INTERP` harness on `doge bark`)
//! and its stdout must match the committed `.out` byte for byte — the same output
//! the compiled binary produces. This is the guard that the two execution engines
//! never drift apart.

use std::path::PathBuf;
use std::process::Command;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

#[test]
fn examples_run_identically_under_the_interpreter() {
    let dir = examples_dir();
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
