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
fn bark_prints_a_class_value_and_it_constructs() {
    // class_value.doge uses a bare class name as a value — a callable that `bark`
    // prints as `<class Name>` and that builds an instance when called.
    let fixture = cli_fixtures_dir().join("class_value.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert!(output.status.success(), "a class value runs cleanly");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert_eq!(stdout, "<class Shibe>\nbork\n");
}

#[test]
fn bark_forwards_command_line_arguments_to_the_script() {
    // Everything after the script path reaches the program through `env.args()`.
    let fixture = cli_fixtures_dir().join("args_echo.doge");
    let output = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .arg("alpha")
        .arg("beta")
        .output()
        .expect("the doge binary should run");

    assert!(
        output.status.success(),
        "a script reading args runs cleanly, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert_eq!(stdout, "alpha\nbeta\n", "env.args() should echo both args");
}

#[test]
fn gib_reads_a_line_of_input_and_none_at_end() {
    use std::io::Write;
    use std::process::Stdio;

    // Two lines then EOF: the first two gib calls read them, the third is `none`.
    let fixture = cli_fixtures_dir().join("gib_echo.doge");
    let mut child = doge_cached()
        .arg("bark")
        .arg(&fixture)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the doge binary should start");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(b"kabosu\ndoge\n")
        .expect("write the input");
    let output = child.wait_with_output().expect("the script should finish");

    assert!(
        output.status.success(),
        "a script reading input runs cleanly, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    // The prompt prints inline before the first bark; EOF yields `none`.
    assert_eq!(stdout, "name? much hello kabosu\ndoge\nnone\n");
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

#[test]
fn new_scaffolds_a_project_that_runs() {
    // `doge new` creates a project; a bare `doge bark` inside it runs the entry.
    let workdir = PathBuf::from(concat!(env!("CARGO_TARGET_TMPDIR"), "/new-project"));
    let _ = std::fs::remove_dir_all(&workdir);
    std::fs::create_dir_all(&workdir).expect("scratch dir");

    let created = doge()
        .arg("new")
        .arg("demo")
        .current_dir(&workdir)
        .output()
        .expect("the doge binary should run");
    assert!(created.status.success(), "doge new should exit 0");
    let project = workdir.join("demo");
    assert!(project.join("doge.toml").is_file(), "a manifest is written");
    assert!(project.join("main.doge").is_file(), "an entry is written");

    let run = doge_cached()
        .arg("bark")
        .current_dir(&project)
        .output()
        .expect("the doge binary should run");
    assert!(
        run.status.success(),
        "the scaffolded project should run, stderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8(run.stdout).expect("utf-8 stdout");
    assert!(
        stdout.contains("much hello from demo"),
        "the scaffold greets, got:\n{stdout}"
    );
}

#[test]
fn new_refuses_to_overwrite_an_existing_directory() {
    let workdir = PathBuf::from(concat!(env!("CARGO_TARGET_TMPDIR"), "/new-existing"));
    let _ = std::fs::remove_dir_all(&workdir);
    std::fs::create_dir_all(workdir.join("taken")).expect("scratch dir");

    let output = doge()
        .arg("new")
        .arg("taken")
        .current_dir(&workdir)
        .output()
        .expect("the doge binary should run");
    assert_eq!(output.status.code(), Some(1), "an occupied name exits 1");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("very exists. much occupied."),
        "should be doge-flavored, got:\n{stderr}"
    );
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

#[test]
fn test_runs_a_file_and_ignores_non_test_functions() {
    let fixture = cli_fixtures_dir().join("tests_pass.doge");
    let output = doge()
        .arg("test")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert!(output.status.success(), "all tests pass, so exit 0");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert!(
        stdout.contains("✓ test_addition") && stdout.contains("✓ test_greeting"),
        "should report each test as passing, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("greet") || stdout.contains("test_greeting"),
        "the `greet` helper is not a test and must not run, got:\n{stdout}"
    );
    assert!(
        stdout.contains("2 passed"),
        "should aggregate a pass count, got:\n{stdout}"
    );
}

#[test]
fn test_reports_a_failing_assertion_with_location() {
    let fixture = cli_fixtures_dir().join("tests_fail.doge");
    let output = doge()
        .arg("test")
        .arg(&fixture)
        .output()
        .expect("the doge binary should run");

    assert_eq!(output.status.code(), Some(1), "a failing test exits 1");
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert!(
        stdout.contains("✓ test_ok"),
        "the passing test still runs, got:\n{stdout}"
    );
    assert!(
        stdout.contains("✗ test_broken") && stdout.contains("one is not two"),
        "should name the failing test and its message, got:\n{stdout}"
    );
    assert!(
        stdout.contains("tests_fail.doge:6"),
        "should carry the failing assertion's location, got:\n{stdout}"
    );
    assert!(
        stdout.contains("1 passed, 1 failed"),
        "should aggregate pass/fail counts, got:\n{stdout}"
    );
}

#[test]
fn test_discovers_test_files_recursively_in_a_directory() {
    let dir = cli_fixtures_dir().join("test_suite");
    let output = doge()
        .arg("test")
        .arg(&dir)
        .output()
        .expect("the doge binary should run");

    assert!(
        output.status.success(),
        "every discovered test passes, exit 0"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert!(
        stdout.contains("✓ test_a") && stdout.contains("✓ test_b") && stdout.contains("✓ test_c"),
        "should discover test_*.doge at any depth, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("test_should_be_ignored"),
        "helper.doge is not a test_*.doge file and must be skipped, got:\n{stdout}"
    );
    assert!(
        stdout.contains("3 passed"),
        "should aggregate across files, got:\n{stdout}"
    );
}

#[test]
fn test_with_no_test_functions_reports_an_empty_suite() {
    let hello = examples_dir().join("hello.doge");
    let output = doge()
        .arg("test")
        .arg(&hello)
        .output()
        .expect("the doge binary should run");

    assert_eq!(
        output.status.code(),
        Some(1),
        "an empty suite exits non-zero"
    );
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("very empty. much untested."),
        "should be a doge-flavored empty-suite message, got:\n{stderr}"
    );
}
