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
fn a_default_and_variadic_header_checks_cleanly() {
    assert!(
        check_src("such f much a, b = 2, many rest:\n    return a\nwow\nbark f(1)\nwow\n").is_ok()
    );
}

#[test]
fn duplicate_parameter_names_are_rejected() {
    let err = check_src("such f much a, a:\n    return a\nwow\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
    let variadic = check_src("such f much a, many a:\n    return a\nwow\nwow\n").unwrap_err();
    assert_eq!(variadic.headline, "very twice. much name.");
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
fn augmented_assign_to_undeclared_is_an_error() {
    let err = check_src("count += 1\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very undeclared. much assign.");
}

#[test]
fn augmented_assign_to_a_const_is_an_error() {
    let err = check_src("so N = 3\nN += 1\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very const. much fixed.");
}

#[test]
fn augmented_assign_to_a_declared_name_passes() {
    assert!(check_src("such n = 1\nn += 2\nbark n\nwow\n").is_ok());
}

#[test]
fn destructuring_declaration_binds_every_name() {
    assert!(check_src(
        "such xs = [1, 2, 3]\nsuch a, b, many rest = xs\nbark a\nbark b\nbark rest\nwow\n"
    )
    .is_ok());
}

#[test]
fn destructuring_reassignment_needs_declared_targets() {
    let err = check_src("such a = 1\nsuch xs = [1, 2]\na, b = xs\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very undeclared. much assign.");
}

#[test]
fn repeated_destructuring_name_is_an_error() {
    let err = check_src("such xs = [1, 2]\nsuch a, a = xs\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very twice. much name.");
}

#[test]
fn for_loop_destructuring_binds_its_variables() {
    assert!(
        check_src("such d = {\"a\": 1}\nfor k, v in d:\n    bark k\n    bark v\nwow\n").is_ok()
    );
}

#[test]
fn slice_and_ternary_resolve_their_names() {
    assert!(check_src("such xs = [1, 2, 3]\nbark xs[0:2]\nwow\n").is_ok());
    // A name used only inside a slice bound must still be declared.
    let err = check_src("such xs = [1]\nbark xs[lo:hi]\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very unknown. much name.");
    // Both ternary branches are checked.
    let err = check_src("bark 1 if true else missing\nwow\n").unwrap_err();
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

#[test]
fn inheritance_from_an_unknown_class_is_an_error() {
    let src = "many Corgi much Nope:\n    such go:\n        return 1\n    wow\nwow\nwow\n";
    let err = check_src(src).unwrap_err();
    assert_eq!(err.headline, "very parent. much unknown.");
}

#[test]
fn an_inheritance_cycle_is_an_error() {
    let src = "many A much B:\n    such g:\n        return 1\n    wow\nwow\nmany B much A:\n    such h:\n        return 2\n    wow\nwow\nwow\n";
    let err = check_src(src).unwrap_err();
    assert_eq!(err.headline, "very loop. much family.");
}

#[test]
fn clean_inheritance_with_super_passes() {
    let src = "many A:\n    such init much n:\n        self.n = n\n    wow\n    such go:\n        return self.n\n    wow\nwow\nmany B much A:\n    such go:\n        return super.go()\n    wow\nwow\nsuch b = B(1)\nbark b.go()\nwow\n";
    assert!(check_src(src).is_ok());
}

#[test]
fn super_outside_a_method_is_an_error() {
    let src = "such f:\n    return super.foo()\nwow\nwow\n";
    let err = check_src(src).unwrap_err();
    assert_eq!(err.headline, "very super. much lost.");
}

#[test]
fn super_in_a_class_without_a_parent_is_an_error() {
    let src = "many A:\n    such go:\n        return super.foo()\n    wow\nwow\nwow\n";
    let err = check_src(src).unwrap_err();
    assert_eq!(err.headline, "very super. much orphan.");
}

#[test]
fn super_to_an_unknown_method_is_an_error() {
    let src = "many A:\n    such go:\n        return 1\n    wow\nwow\nmany B much A:\n    such go2:\n        return super.nope()\n    wow\nwow\nwow\n";
    let err = check_src(src).unwrap_err();
    assert_eq!(err.headline, "very super. much unknown.");
}
