use std::path::PathBuf;
use std::process::{Command, Stdio};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn cache_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_TARGET_TMPDIR"), "/doge-cache"))
}

/// A scratch working directory for the examples, so one that touches files
/// (`io.doge`) does so on a relative path here instead of the source tree. Named
/// per engine so the compiled and interpreted suites never collide on a file.
fn run_cwd() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_TARGET_TMPDIR"), "/compiled-cwd"))
}

#[test]
fn examples_with_expected_output_run_and_match() {
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

/// Project examples: a subdirectory with a `doge.toml` and an `expected.out`.
/// Each is run via a bare `doge bark` from inside the project, so its manifest
/// entry and declared dependencies are resolved exactly as a user's would be.
#[test]
fn project_examples_run_and_match() {
    let dir = examples_dir();
    let mut ran = 0;
    for entry in std::fs::read_dir(&dir).expect("examples directory should exist") {
        let project = entry.expect("readable dir entry").path();
        if !project.is_dir() {
            continue;
        }
        if !project.join("doge.toml").is_file() {
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
            .env("DOGE_CACHE_DIR", cache_dir())
            .output()
            .expect("the doge binary should run");

        let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{} should run cleanly, exit={:?}\nstderr:\n{stderr}",
            project.display(),
            output.status.code(),
        );
        assert_eq!(
            stdout,
            expected,
            "{} stdout should match its expected.out",
            project.display()
        );
    }
    assert!(ran > 0, "no runnable project examples were found");
}
