use super::*;
use crate::parser::parse;

fn check_src(source: &str) -> Result<(), Diagnostic> {
    let script = parse("test.doge", source).expect("parse should succeed");
    check("test.doge", source, &script)
}

#[test]
fn clean_program_passes() {
    assert!(check_src("such x = 1\nbark x\nwow\n").is_ok());
}

#[test]
fn assign_to_undeclared_is_an_error() {
    let err = check_src("x = 1\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very undeclared. much assign.");
}

#[test]
fn very_assign_to_undeclared_is_an_error() {
    let err = check_src("very x = 1\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very undeclared. much assign.");
}

#[test]
fn reassigning_a_const_is_an_error() {
    let err = check_src("so PI = 3\nPI = 4\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very const. much fixed.");
}

#[test]
fn reading_an_undeclared_name_is_an_error() {
    let err = check_src("bark nope\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very unknown. much name.");
}

#[test]
fn bork_outside_loop_is_an_error() {
    let err = check_src("bork\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very bork. much nowhere.");
}

#[test]
fn continue_outside_loop_is_an_error() {
    let err = check_src("continue\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very continue. much nowhere.");
}

#[test]
fn return_outside_function_is_an_error() {
    let err = check_src("return 1\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very return. much lost.");
}

#[test]
fn bork_inside_loop_is_fine() {
    assert!(check_src("such xs = [1]\nfor x in xs:\n    bork\nwow\n").is_ok());
}

#[test]
fn return_inside_function_is_fine() {
    assert!(check_src("such f:\n    return 1\nwow\nwow\n").is_ok());
}

#[test]
fn mutual_recursion_is_allowed() {
    // `a` calls `b`, defined later; both are top-level names via the pre-pass.
    let src = "such a:\n    b()\nwow\nsuch b:\n    a()\nwow\nwow\n";
    assert!(check_src(src).is_ok());
}

#[test]
fn params_and_self_are_in_scope() {
    let func = "such greet much name:\n    bark name\nwow\nwow\n";
    assert!(check_src(func).is_ok());
    let method = "many Shibe:\n    such speak:\n        bark self\n    wow\nwow\nwow\n";
    assert!(check_src(method).is_ok());
}

#[test]
fn builtin_names_are_known() {
    assert!(check_src("such xs = [1]\nbark len(xs)\nwow\n").is_ok());
}

#[test]
fn duplicate_function_names_are_an_error() {
    let err = check_src("such f:\n    bark 1\nwow\nsuch f:\n    bark 2\nwow\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
}

#[test]
fn function_clashing_with_a_variable_is_an_error() {
    let err = check_src("such x = 1\nsuch x:\n    bark 1\nwow\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
}

#[test]
fn function_named_like_a_builtin_is_an_error() {
    let err = check_src("such len:\n    bark 1\nwow\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
    assert!(err.message.contains("builtin"));
}

#[test]
fn duplicate_class_names_are_an_error() {
    let err =
            check_src("many Shibe:\n    such a:\n        bark 1\n    wow\nwow\nmany Shibe:\n    such b:\n        bark 2\n    wow\nwow\nwow\n")
                .unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
}

#[test]
fn class_named_like_a_builtin_is_an_error() {
    let err = check_src("many len:\n    such a:\n        bark 1\n    wow\nwow\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
    assert!(err.message.contains("builtin"));
}

#[test]
fn import_clashing_with_a_variable_is_an_error() {
    let err = check_src("such nerd = 1\nso nerd\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
}

#[test]
fn importing_the_same_module_twice_is_an_error() {
    let err = check_src("so nerd\nso nerd\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
}

#[test]
fn duplicate_method_in_one_class_is_an_error() {
    let err = check_src(
            "many Shibe:\n    such speak:\n        bark 1\n    wow\n    such speak:\n        bark 2\n    wow\nwow\nwow\n",
        )
        .unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
    assert!(err.message.contains("method"));
}

#[test]
fn top_level_use_before_declaration_is_an_error() {
    // `y` is a top-level name, but used before its declaration line.
    let err = check_src("bark y\nsuch y = 1\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very unknown. much name.");
}

#[test]
fn nested_function_sees_enclosing_locals() {
    // `inner` reads and writes `count`, a local of `outer`.
    let src = "such outer:\n    such count = 0\n    such inner:\n        very count = count + 1\n    wow\n    inner()\nwow\nwow\n";
    assert!(check_src(src).is_ok());
}

#[test]
fn nested_sibling_functions_can_call_each_other() {
    // `a` calls `b`, defined later in the same body — forward reference.
    let src = "such outer:\n    such a:\n        b()\n    wow\n    such b:\n        bark 1\n    wow\n    a()\nwow\nwow\n";
    assert!(check_src(src).is_ok());
}

#[test]
fn unknown_name_in_a_nested_function_is_still_an_error() {
    let src = "such outer:\n    such inner:\n        bark nope\n    wow\nwow\nwow\n";
    let err = check_src(src).unwrap_err();
    assert_eq!(err.headline, "very unknown. much name.");
}

#[test]
fn nested_function_clashing_with_a_local_is_an_error() {
    let src = "such outer:\n    such x = 1\n    such x:\n        bark 1\n    wow\nwow\nwow\n";
    let err = check_src(src).unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
}

#[test]
fn bork_inside_a_nested_function_cannot_cross_the_outer_loop() {
    // The loop is in `outer`; `inner`'s body is a fresh function scope, so
    // `bork` has no loop to break.
    let src = "such outer:\n    such xs = [1]\n    for x in xs:\n        such inner:\n            bork\n        wow\n    bark 1\nwow\nwow\n";
    let err = check_src(src).unwrap_err();
    assert_eq!(err.headline, "very bork. much nowhere.");
}
