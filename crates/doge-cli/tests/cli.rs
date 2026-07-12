//! End-to-end tests for the `doge` binary: they spawn the real compiled
//! executable (via `CARGO_BIN_EXE_doge`) the way a user would.

use std::path::PathBuf;
use std::process::Command;

fn doge() -> Command {
    Command::new(env!("CARGO_BIN_EXE_doge"))
}

/// A `doge` invocation that caches under `CARGO_TARGET_TMPDIR`, out of the real
/// user cache. Shared with `run_examples.rs`, so the runtime compiles once.
fn doge_cached() -> Command {
    let mut cmd = doge();
    cmd.env("DOGE_CACHE_DIR", cache_dir());
    cmd
}

fn cache_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_TARGET_TMPDIR"), "/doge-cache"))
}

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

/// The compiler's check fixtures, reused for the `check` diagnostic test.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("doge-compiler")
        .join("tests")
        .join("fixtures")
}

/// This crate's own test fixtures (the runtime-error script).
fn cli_fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn module_fixtures_dir() -> PathBuf {
    cli_fixtures_dir().join("modules")
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
fn an_unknown_command_prints_usage_and_exits_two() {
    // A bare `doge` now starts the REPL, so usage is reported for an unrecognized
    // subcommand instead.
    let output = doge()
        .arg("frobnicate")
        .arg("x.doge")
        .output()
        .expect("the doge binary should run");
    assert_eq!(output.status.code(), Some(2), "expected exit 2");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("such usage"),
        "should print usage, got:\n{stderr}"
    );
}

#[test]
fn bark_runs_and_caches() {
    let hello = examples_dir().join("hello.doge");
    let expected = std::fs::read_to_string(examples_dir().join("hello.out")).expect("hello.out");

    let first = doge_cached()
        .arg("bark")
        .arg(&hello)
        .output()
        .expect("the doge binary should run");
    assert!(first.status.success(), "first run should exit 0");
    assert_eq!(String::from_utf8_lossy(&first.stdout), expected);

    // After the first build, the script's cache entry must exist on disk.
    let scripts = cache_dir().join("scripts");
    assert!(
        scripts.exists() && scripts.read_dir().expect("scripts dir").next().is_some(),
        "a cache entry should have been written under {}",
        scripts.display()
    );

    // The second run is a cache hit and must produce identical output.
    let second = doge_cached()
        .arg("bark")
        .arg(&hello)
        .output()
        .expect("the doge binary should run");
    assert!(second.status.success(), "second run should exit 0");
    assert_eq!(String::from_utf8_lossy(&second.stdout), expected);
}

#[test]
fn runtime_error_reports_path_line_and_source() {
    let fixture = cli_fixtures_dir().join("divide_by_zero.doge");
    let source_line = std::fs::read_to_string(&fixture)
        .expect("divide_by_zero.doge")
        .lines()
        .nth(2)
        .expect("line 3")
        .to_string();

    let output = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "a runtime error exits 1");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very error. much broken."),
        "should be doge-flavored, got:\n{stderr}"
    );
    assert!(
        stderr.contains("divide_by_zero.doge:3"),
        "should carry the script path and line, got:\n{stderr}"
    );
    assert!(
        stderr.contains("by zero"),
        "should explain the division error, got:\n{stderr}"
    );
    assert!(
        stderr.contains(&source_line),
        "should embed the offending line-3 source ({source_line:?}), got:\n{stderr}"
    );
}

#[test]
fn caught_error_exposes_type_file_and_line() {
    let fixture = cli_fixtures_dir().join("caught_error_fields.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert!(
        output.status.success(),
        "a caught error runs cleanly, exit={:?}",
        output.status.code()
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert!(
        stdout.contains("IndexOutOfBounds"),
        "err.type names the category, got:\n{stdout}"
    );
    assert!(
        stdout.contains("caught_error_fields.doge"),
        "err.file carries the script path, got:\n{stdout}"
    );
    assert!(
        stdout.lines().any(|line| line == "3"),
        "err.line is the raise line (3), got:\n{stdout}"
    );
}

#[test]
fn bark_prints_a_function_value() {
    // func_value.doge uses a bare function name as a value — now a first-class
    // function that `bark` prints as `<function name>`.
    let fixture = cli_fixtures_dir().join("func_value.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert!(output.status.success(), "a function value runs cleanly");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert_eq!(stdout, "<function shout>\n");
}

#[test]
fn bark_on_a_class_used_as_a_value_is_an_error() {
    // class_value.doge uses an object definition as a value — a class is not a
    // first-class value, you call it to build an instance.
    let fixture = cli_fixtures_dir().join("class_value.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert_eq!(
        output.status.code(),
        Some(1),
        "an unsupported feature exits 1"
    );
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very class. much value."),
        "should say a class is not a value, got:\n{stderr}"
    );
}

#[test]
fn uncaught_bonk_reports_path_and_line() {
    let fixture = cli_fixtures_dir().join("bonk.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "an uncaught bonk exits 1");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert_eq!(stdout, "before\n", "the bark before the bonk still runs");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very error. much broken."),
        "should be doge-flavored, got:\n{stderr}"
    );
    assert!(
        stderr.contains("bonk.doge:2"),
        "should carry the script path and line, got:\n{stderr}"
    );
    assert!(
        stderr.contains("such bad"),
        "should show the bonked message, got:\n{stderr}"
    );
}

#[test]
fn uncaught_amaze_reports_path_line_and_message() {
    let fixture = cli_fixtures_dir().join("amaze_fail.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "a failed amaze exits 1");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert_eq!(stdout, "before\n", "the bark before the amaze still runs");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very error. much broken."),
        "should be doge-flavored, got:\n{stderr}"
    );
    assert!(
        stderr.contains("amaze_fail.doge:3"),
        "should carry the script path and line, got:\n{stderr}"
    );
    assert!(
        stderr.contains("age much wrong"),
        "should show the amaze message, got:\n{stderr}"
    );
}

#[test]
fn recursion_limit_is_a_catchable_doge_error() {
    let fixture = cli_fixtures_dir().join("deep_recursion.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "runaway recursion exits 1");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very error. much broken."),
        "should be doge-flavored, got:\n{stderr}"
    );
    assert!(
        stderr.contains("too much recursion"),
        "should explain the recursion limit, got:\n{stderr}"
    );
    // The Rust stack never overflows — the user sees no rustc/abort noise.
    assert!(
        !stderr.contains("stack overflow") && !stderr.contains("panicked"),
        "no Rust abort should leak, got:\n{stderr}"
    );
}

#[test]
fn build_produces_standalone_binary() {
    // Its own fixture, not hello.doge: the cache is keyed by source, and another
    // test builds hello.doge concurrently — sharing a source would race two
    // parallel builds on the same cached binary.
    let script = cli_fixtures_dir()
        .join("standalone.doge")
        .canonicalize()
        .expect("standalone.doge should exist");
    let expected = "much standalone. very wow.\n";
    let workdir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));

    let build = doge_cached()
        .arg("build")
        .arg(&script)
        .current_dir(&workdir)
        .output()
        .expect("the doge binary should run");
    assert!(
        build.status.success(),
        "build should exit 0, stderr:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let dropped = workdir.join(format!("standalone{}", std::env::consts::EXE_SUFFIX));
    assert!(
        dropped.exists(),
        "build should drop ./standalone in the work dir"
    );

    // The dropped binary runs standalone, with no doge involvement.
    let run = Command::new(&dropped)
        .output()
        .expect("the built binary should run");
    assert!(run.status.success(), "the standalone binary should exit 0");
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
}

#[test]
fn concurrent_builds_of_one_script_all_succeed() {
    // Several `doge` processes building the same script share one cache entry.
    // Without the build lock, one process's cargo relink races another's run
    // ("os error 2"); with it, one builds and the rest reuse the binary.
    let fixture = cli_fixtures_dir().join("concurrent.doge");
    let expected = "much concurrent. very safe.\n";

    let racers: Vec<_> = (0..4)
        .map(|_| {
            let fixture = fixture.clone();
            std::thread::spawn(move || {
                doge_cached()
                    .arg("bark")
                    .arg(&fixture)
                    .output()
                    .expect("the doge binary should run")
            })
        })
        .collect();

    for racer in racers {
        let output = racer.join().expect("a build thread should not panic");
        assert!(
            output.status.success(),
            "every concurrent build should exit 0, stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
    }
}

#[test]
fn bark_runs_a_program_with_imported_modules() {
    let entry = examples_dir().join("modules.doge");
    let expected =
        std::fs::read_to_string(examples_dir().join("modules.out")).expect("modules.out");

    let output = doge_cached()
        .arg("bark")
        .arg(&entry)
        .output()
        .expect("the doge binary should run");

    assert!(
        output.status.success(),
        "a multi-file program should run, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

#[test]
fn runtime_error_in_a_module_reports_that_module_and_line() {
    let entry = module_fixtures_dir().join("rterr_entry.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&entry)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "a runtime error exits 1");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    // The error surfaces the *module's* file and line, not the entry's.
    assert!(
        stderr.contains("rterr_lib.doge:2"),
        "should point into the module, got:\n{stderr}"
    );
    assert!(
        stderr.contains("by zero"),
        "should explain the division error, got:\n{stderr}"
    );
}

#[test]
fn importing_a_missing_module_is_a_doge_diagnostic() {
    let entry = module_fixtures_dir().join("missing_entry.doge");
    let output = doge()
        .arg("check")
        .arg(&entry)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "an unknown module exits 1");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very import. much unknown."),
        "should be doge-flavored, got:\n{stderr}"
    );
    assert!(
        stderr.contains("no module named nope"),
        "should name the missing module, got:\n{stderr}"
    );
}

#[test]
fn a_circular_import_is_a_doge_diagnostic() {
    let entry = module_fixtures_dir().join("cycle_entry.doge");
    let output = doge()
        .arg("check")
        .arg(&entry)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "a cycle exits 1");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very loop. much import."),
        "should be doge-flavored, got:\n{stderr}"
    );
    assert!(
        stderr.contains("import cycle:"),
        "should spell out the cycle, got:\n{stderr}"
    );
}

#[test]
fn a_loose_statement_in_a_module_is_a_doge_diagnostic() {
    let entry = module_fixtures_dir().join("loose_entry.doge");
    let output = doge()
        .arg("check")
        .arg(&entry)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "a loose statement exits 1");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very loose. much module."),
        "should be doge-flavored, got:\n{stderr}"
    );
}

#[test]
fn an_object_defined_in_a_module_is_importable() {
    let entry = module_fixtures_dir().join("obj_entry.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&entry)
        .output()
        .expect("the doge binary should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "a module object should construct and dispatch, got:\n{stderr}"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert_eq!(stdout, "1\n", "utils.Shibe().woof() should print 1");
}

/// Drive the interactive REPL by piping a scripted session into it and asserting
/// on the echoed values and printed output.
fn repl_session(input: &str) -> String {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = doge()
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the doge binary should start");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input.as_bytes())
        .expect("write the session");
    let output = child.wait_with_output().expect("the repl should finish");
    String::from_utf8(output.stdout).expect("utf-8 stdout")
}

#[test]
fn repl_runs_a_session_and_persists_bindings() {
    // Declare a variable, use it, redefine it, and echo a trailing expression —
    // across separate lines, so the session must persist state between them.
    let stdout = repl_session("such x = 20\nbark x + 1\nsuch x = 100\nx * 2\nwow\n");
    assert!(
        stdout.contains("21"),
        "bark should print 21, got:\n{stdout}"
    );
    assert!(
        stdout.contains("200"),
        "the trailing expression should echo 200 after redefinition, got:\n{stdout}"
    );
}

#[test]
fn repl_recovers_after_an_error() {
    // An unknown name reports an error, then the session keeps going.
    let stdout = repl_session("bark nope\nbark 7\nwow\n");
    assert!(
        stdout.contains("7"),
        "the session should continue after an error, got:\n{stdout}"
    );
}

#[test]
fn a_bare_doge_starts_the_repl() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = doge()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the doge binary should start");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(b"bark 42\nwow\n")
        .expect("write the session");
    let output = child.wait_with_output().expect("the repl should finish");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert!(
        stdout.contains("42"),
        "a bare `doge` should start the repl, got:\n{stdout}"
    );
}

/// A scratch file under the target tmp dir, seeded with `content`. `doge fmt`
/// rewrites in place, so tests must point it at throwaway files, never at a
/// checked-in fixture.
fn scratch_file(name: &str, content: &str) -> PathBuf {
    let path = PathBuf::from(concat!(env!("CARGO_TARGET_TMPDIR"), "/fmt")).join(name);
    std::fs::create_dir_all(path.parent().expect("scratch parent")).expect("mkdir scratch");
    std::fs::write(&path, content).expect("seed scratch file");
    path
}

const UNFORMATTED: &str = "such xs=[1,2 , 3]\nbark  xs[ 0 ]\nwow\n";
const FORMATTED: &str = "such xs = [1, 2, 3]\nbark xs[0]\nwow\n";

#[test]
fn fmt_rewrites_a_file_in_place() {
    let path = scratch_file("rewrite.doge", UNFORMATTED);
    let output = doge()
        .arg("fmt")
        .arg(&path)
        .output()
        .expect("doge should run");
    assert!(output.status.success(), "fmt should exit 0");
    assert_eq!(
        std::fs::read_to_string(&path).expect("read back"),
        FORMATTED
    );
}

#[test]
fn fmt_check_fails_on_unformatted_and_leaves_the_file() {
    let path = scratch_file("check_bad.doge", UNFORMATTED);
    let output = doge()
        .arg("fmt")
        .arg("--check")
        .arg(&path)
        .output()
        .expect("doge should run");
    assert_eq!(output.status.code(), Some(1), "expected exit 1");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("very"), "doge-flavored, got:\n{stderr}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("read back"),
        UNFORMATTED,
        "--check must not write"
    );
}

#[test]
fn fmt_check_passes_on_a_formatted_file() {
    let path = scratch_file("check_good.doge", FORMATTED);
    let output = doge()
        .arg("fmt")
        .arg("--check")
        .arg(&path)
        .output()
        .expect("doge should run");
    assert!(output.status.success(), "already-formatted should exit 0");
}

#[test]
fn fmt_on_unparseable_source_reports_a_diagnostic() {
    let path = scratch_file("broken.doge", "such =\nwow\n");
    let output = doge()
        .arg("fmt")
        .arg(&path)
        .output()
        .expect("doge should run");
    assert_eq!(output.status.code(), Some(1), "expected exit 1");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("very"),
        "doge-flavored diagnostic, got:\n{stderr}"
    );
}
