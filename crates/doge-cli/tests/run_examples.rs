use std::path::PathBuf;
use std::process::Command;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn cache_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_TARGET_TMPDIR"), "/doge-cache"))
}

#[test]
fn examples_with_expected_output_run_and_match() {
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
            .env("DOGE_CACHE_DIR", cache_dir())
            .output()
            .expect("the doge binary should run");

        let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{} should run cleanly, exit={:?}\nstderr:\n{stderr}",
            doge.display(),
            output.status.code(),
        );
        assert_eq!(
            stdout,
            expected,
            "{} stdout should match its .out",
            doge.display()
        );
    }
    assert!(ran > 0, "no runnable examples with .out files were found");
}
