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
        run_program(std::sync::Arc::new(program))
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
fn class_name_is_a_first_class_value() {
    // A bare class name evaluates to a callable class value that prints distinctly.
    let source = "\
many Shibe:
    such speak:
        return \"bork\"
    wow
wow
such f = Shibe
f
";
    assert_eq!(eval(source), "<class Shibe>");
}

#[test]
fn class_values_compare_by_identity() {
    let source = "\
many Shibe:
    such go:
        return 1
    wow
wow
many Corgi:
    such go:
        return 1
    wow
wow
[Shibe == Shibe, Shibe == Corgi]
";
    assert_eq!(eval(source), "[true, false]");
}

#[test]
fn a_class_value_in_a_collection_constructs_an_instance() {
    // The factory pattern from the issue: store classes, call one to build an
    // instance, and read the field its `init` set.
    let source = "\
many Shibe:
    such init much name:
        self.name = name
    wow
wow
such factories = [Shibe]
such pet = factories[0](\"kabosu\")
pet.name
";
    assert_eq!(eval(source), "kabosu");
}

#[test]
fn a_class_value_with_no_init_constructs_from_no_arguments() {
    let source = "\
many Empty:
    such tag:
        return \"t\"
    wow
wow
such make = Empty
make().tag()
";
    assert_eq!(eval(source), "t");
}

#[test]
fn calling_a_class_value_with_wrong_arity_is_catchable() {
    let source = "\
many Shibe:
    such init much name:
        self.name = name
    wow
wow
such f = Shibe
pls
    f()
oh no err!
    such kind = err.type
kind
";
    assert_eq!(eval(source), "TypeError");
}

#[test]
fn method_read_as_a_value_is_a_bound_method() {
    // A method read off an instance prints as a bound method and calls back to it.
    let source = "\
many Shibe:
    such init much name:
        self.name = name
    wow
    such speak:
        return self.name + \" says bork\"
    wow
wow
such a = Shibe(\"kabosu\")
such say = a.speak
[say(), a.speak]
";
    assert_eq!(eval(source), "[\"kabosu says bork\", <method Shibe.speak>]");
}

#[test]
fn bound_collection_method_mutates_its_receiver() {
    let source = "\
such xs = [1, 2]
such push = xs.append
push(3)
xs
";
    assert_eq!(eval(source), "[1, 2, 3]");
}

#[test]
fn bound_methods_compare_by_receiver_and_name() {
    let source = "\
many Shibe:
    such speak:
        return 1
    wow
wow
such a = Shibe()
such b = Shibe()
[a.speak == a.speak, a.speak == b.speak]
";
    assert_eq!(eval(source), "[true, false]");
}

#[test]
fn a_field_shadows_a_method_of_the_same_name() {
    let source = "\
many Shibe:
    such speak:
        return 1
    wow
wow
such a = Shibe()
a.speak = \"field\"
a.speak
";
    assert_eq!(eval(source), "field");
}

#[test]
fn reading_a_missing_name_off_an_object_is_catchable() {
    let source = "\
many Shibe:
    such speak:
        return 1
    wow
wow
such a = Shibe()
a.fly
wow
";
    assert_eq!(run_err(source), ErrorKind::AttrError);
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

#[test]
fn prepare_then_call_entry_function_drives_tests() {
    let source =
        "such test_ok:\n    amaze 1 + 1 == 2\nwow\nsuch test_bad:\n    amaze false\nwow\nwow\n"
            .to_string();
    on_big_stack(move || {
        let program = dc::load_program("test.doge", &source).expect("parses and loads");
        dc::check_program(&program).expect("checks");
        let mut interp = Interp::new();
        interp
            .prepare(std::sync::Arc::new(program))
            .expect("integrates cleanly");
        assert!(
            interp.call_entry_function("test_ok").is_ok(),
            "a passing test returns Ok"
        );
        assert_eq!(
            interp
                .call_entry_function("test_bad")
                .expect_err("a failing amaze raises")
                .kind,
            ErrorKind::AssertError
        );
    });
}

/// Run a whole program through the interpreter, returning the kind of any uncaught
/// error (the `Send` part of the error, so it can leave the big-stack thread). Used
/// for `pack` tests, which assert inside the script with `amaze` so a mismatch
/// becomes an uncaught error.
fn run(source: &str) -> Result<(), ErrorKind> {
    let source = source.to_string();
    on_big_stack(move || {
        let program = dc::load_program("test.doge", &source).expect("parses and loads");
        dc::check_program(&program).expect("checks");
        run_program(std::sync::Arc::new(program)).map_err(|err| err.kind)
    })
}

#[test]
fn pack_zoom_runs_a_pup_and_fetch_returns_its_result() {
    run("so pack\nsuch sq much n:\n    return n * n\nwow\nsuch p = pack.zoom(sq, [6])\namaze pack.fetch(p) == 36\nwow\n")
        .expect("the pup computes 36 and fetch returns it");
}

#[test]
fn a_pups_error_is_re_raised_by_fetch_in_the_interpreter() {
    // The pup bonks; fetch re-raises it, and pls/oh no on the fetch catches it.
    run("so pack\nsuch boom much n:\n    bonk \"nope\"\nwow\nsuch p = pack.zoom(boom, [1])\npls\n    such r = pack.fetch(p)\noh no e!\n    amaze e.message == \"nope\"\n\nwow\n")
        .expect("the pup error is caught with its message intact");
}

#[test]
fn a_bowl_passes_a_value_between_pups_in_the_interpreter() {
    run("so pack\nsuch feed much b:\n    pack.drop(b, 7)\n    return 0\nwow\nsuch b = pack.bowl()\nsuch p = pack.zoom(feed, [b])\namaze pack.sniff(b) == 7\nsuch done = pack.fetch(p)\nwow\n")
        .expect("a value dropped in a pup is sniffed on the main thread");
}
