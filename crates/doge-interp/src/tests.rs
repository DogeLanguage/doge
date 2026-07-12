//! Unit tests for the interpreter: language features exercised end to end through
//! the front end, asserting on returned values and raised errors rather than
//! printed output (the examples parity suite covers stdout).

use super::*;
use doge_runtime::ErrorKind;

/// The interpreter recurses on the native stack, so run every test on a thread
/// with a generous stack — the same guard the CLI applies — and return only the
/// `Send` result. `Rc`-based interpreter state never leaves the thread.
fn on_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .expect("spawns")
        .join()
        .expect("joins")
}

/// Evaluate a REPL-style snippet and return the display form of its trailing
/// expression — the value the prompt would echo.
fn eval(source: &str) -> String {
    let source = source.to_string();
    on_big_stack(move || {
        let mut interp = Interp::new();
        let script = match dc::parse_repl("test.doge", &source) {
            dc::ReplParse::Complete(script) => script,
            _ => panic!("snippet did not parse: {source:?}"),
        };
        dc::check_snippet("test.doge", &source, &script, &SessionScope::empty()).expect("checks");
        interp
            .eval_snippet("test.doge", &script)
            .expect("runs cleanly")
            .expect("has a trailing expression")
            .to_string()
    })
}

/// Run a script expected to raise an uncaught error, returning its kind.
fn run_err(source: &str) -> ErrorKind {
    let source = source.to_string();
    on_big_stack(move || {
        let program = dc::load_program("test.doge", &source).expect("parses and loads");
        dc::check_program(&program).expect("checks");
        run_program(&program)
            .expect_err("expected a runtime error")
            .kind
    })
}

#[test]
fn arithmetic_and_precedence() {
    assert_eq!(eval("2 + 3 * 4\n"), "14");
    assert_eq!(eval("7 / 2\n"), "3.5");
    assert_eq!(eval("7 // 2\n"), "3");
    assert_eq!(eval("2 ** 10\n"), "1024");
}

#[test]
fn strings_and_collections() {
    assert_eq!(eval("\"much \" + \"wow\"\n"), "much wow");
    assert_eq!(eval("len([1, 2, 3])\n"), "3");
    assert_eq!(eval("[1, 2, 3][-1]\n"), "3");
    assert_eq!(eval("{\"a\": 1}[\"a\"]\n"), "1");
}

#[test]
fn short_circuit_and_or_yield_bools() {
    assert_eq!(eval("true and false\n"), "false");
    assert_eq!(eval("false or 5\n"), "true");
    assert_eq!(eval("1 and 0\n"), "false");
}

#[test]
fn closures_capture_and_share_state() {
    let source = "\
such make:
    such n = 0
    such step:
        very n = n + 1
        return n
    wow
    return step
wow
such c = make()
c()
c()
c()
";
    assert_eq!(eval(source), "3");
}

#[test]
fn recursion_computes_and_is_bounded() {
    let fib = "\
such fib much n:
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)
wow
fib(10)
";
    assert_eq!(eval(fib), "55");

    let runaway = "\
such loop much n:
    return loop(n + 1)
wow
loop(0)
wow
";
    assert_eq!(run_err(runaway), ErrorKind::RecursionLimit);
}

#[test]
fn objects_inheritance_and_super() {
    let source = "\
many Animal:
    such init much name:
        self.name = name
    wow
    such speak:
        return self.name + \" makes a sound\"
    wow
wow
many Dog much Animal:
    such speak:
        return super.speak() + \" (woof)\"
    wow
wow
such d = Dog(\"kabosu\")
d.speak()
";
    assert_eq!(eval(source), "kabosu makes a sound (woof)");
}

#[test]
fn try_catch_binds_a_structured_error() {
    let source = "\
pls
    such xs = [1]
    such missing = xs[5]
oh no err!
    such kind = err.type
kind
";
    assert_eq!(eval(source), "IndexOutOfBounds");
}

#[test]
fn destructuring_and_collectors() {
    assert_eq!(eval("such a, b = [1, 2]\nb\n"), "2");
    let source = "such first, many rest = [1, 2, 3, 4]\nrest\n";
    assert_eq!(eval(source), "[2, 3, 4]");
}

#[test]
fn uncaught_errors_carry_their_kind() {
    assert_eq!(run_err("bark 1 / 0\nwow\n"), ErrorKind::DivisionByZero);
    assert_eq!(run_err("bark [1][9]\nwow\n"), ErrorKind::IndexOutOfBounds);
    assert_eq!(run_err("bark 1 + \"x\"\nwow\n"), ErrorKind::TypeError);
    assert_eq!(run_err("amaze false\nwow\n"), ErrorKind::AssertError);
}

#[test]
fn defaults_and_varargs_bind_like_the_compiler() {
    assert_eq!(
        eval("such greet much name, mood = \"ok\":\n    return name + \" is \" + mood\nwow\ngreet(\"kabosu\")\n"),
        "kabosu is ok"
    );
    assert_eq!(
        eval("such tally much many xs:\n    return len(xs)\nwow\ntally(1, 2, 3)\n"),
        "3"
    );
}

#[test]
fn stdlib_is_available() {
    assert_eq!(eval("so nerd\nnerd.sqrt(16.0)\n"), "4.0");
    assert_eq!(eval("so strings\nstrings.beeg(\"wow\")\n"), "WOW");
}
