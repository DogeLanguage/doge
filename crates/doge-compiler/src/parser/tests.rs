use super::*;
use crate::ast::dump;

fn parse_ok(source: &str) -> Script {
    parse("test.doge", source).expect("expected a clean parse")
}

fn parse_err(source: &str) -> Diagnostic {
    parse("test.doge", source).expect_err("expected a parse error")
}

#[test]
fn decl_and_bark() {
    let script = parse_ok("such age = 7\nbark age\nwow\n");
    assert_eq!(script.stmts.len(), 2);
    assert!(matches!(script.stmts[0], Stmt::Decl { .. }));
    assert!(matches!(script.stmts[1], Stmt::Bark { .. }));
}

#[test]
fn such_disambiguates_var_vs_func() {
    let var = parse_ok("such x = 1\nwow\n");
    assert!(matches!(var.stmts[0], Stmt::Decl { .. }));
    let func = parse_ok("such greet much name:\n    bark name\nwow\nwow\n");
    match &func.stmts[0] {
        Stmt::FuncDef { name, params, .. } => {
            assert_eq!(name, "greet");
            assert_eq!(params, &["name".to_string()]);
        }
        other => panic!("expected FuncDef, got {other:?}"),
    }
}

#[test]
fn func_without_params_omits_much() {
    let func = parse_ok("such no_args:\n    bark 1\nwow\nwow\n");
    match &func.stmts[0] {
        Stmt::FuncDef { params, .. } => assert!(params.is_empty()),
        other => panic!("expected FuncDef, got {other:?}"),
    }
}

#[test]
fn so_disambiguates_const_vs_import() {
    let konst = parse_ok("so PI = 3\nwow\n");
    assert!(matches!(konst.stmts[0], Stmt::ConstDecl { .. }));
    let import = parse_ok("so math\nwow\n");
    assert!(matches!(import.stmts[0], Stmt::Import { .. }));
}

#[test]
fn bonk_takes_an_expression() {
    let script = parse_ok("bonk \"x\"\nwow\n");
    let dumped = dump(&script);
    assert!(dumped.contains("Bonk"));
    assert!(dumped.contains("Str \"x\""));
}

#[test]
fn bare_bonk_is_a_parse_error() {
    let err = parse_err("bonk\nwow\n");
    assert!(err.message.contains("expected a value"));
}

#[test]
fn pls_oh_no_shape() {
    let script = parse_ok("pls\n    bark 1\noh no err!\n    bark err\nwow\n");
    match &script.stmts[0] {
        Stmt::Try { err_name, .. } => assert_eq!(err_name, "err"),
        other => panic!("expected Try, got {other:?}"),
    }
}

#[test]
fn objects_hold_methods() {
    let src = "many Shibe:\n    such speak:\n        bark 1\n    wow\nwow\nwow\n";
    let script = parse_ok(src);
    match &script.stmts[0] {
        Stmt::ObjDef { name, methods, .. } => {
            assert_eq!(name, "Shibe");
            assert_eq!(methods.len(), 1);
        }
        other => panic!("expected ObjDef, got {other:?}"),
    }
}

#[test]
fn object_body_rejects_non_methods() {
    let err = parse_err("many Shibe:\n    such x = 1\nwow\nwow\n");
    assert_eq!(err.headline, "very object. much confuse.");
}

#[test]
fn if_elif_else() {
    let script = parse_ok("if a:\n    bark 1\nelif b:\n    bark 2\nelse:\n    bark 3\nwow\n");
    match &script.stmts[0] {
        Stmt::If {
            branches,
            else_body,
            ..
        } => {
            assert_eq!(branches.len(), 2);
            assert!(else_body.is_some());
        }
        other => panic!("expected If, got {other:?}"),
    }
}

#[test]
fn missing_wow_after_function_is_an_error() {
    let err = parse_err("such f:\n    bark 1\n");
    assert_eq!(err.headline, "very incomplete. such missing wow.");
}

#[test]
fn missing_script_wow_is_an_error() {
    let err = parse_err("such x = 1\n");
    assert_eq!(err.headline, "very incomplete. such missing wow.");
}

#[test]
fn extra_after_wow_is_an_error() {
    let err = parse_err("such x = 1\nwow\nbark x\nwow\n");
    assert_eq!(err.headline, "very extra. much after wow.");
}

#[test]
fn chained_comparison_is_an_error() {
    let err = parse_err("bark 1 < x < 10\nwow\n");
    assert!(err.message.contains("chain comparisons"));
}

#[test]
fn def_gets_the_python_hint() {
    let err = parse_err("def greet():\n    bark 1\nwow\n");
    assert_eq!(err.headline, "very python. much habit.");
}

#[test]
fn precedence_mul_over_add() {
    // 1 + 2 * 3  parses as  1 + (2 * 3)
    let script = parse_ok("bark 1 + 2 * 3\nwow\n");
    let dumped = dump(&script);
    assert!(dumped.contains("Binary +"));
    assert!(dumped.contains("Binary *"));
    // The multiply is nested under the add (deeper indentation).
    let add_at = dumped.find("Binary +").unwrap();
    let mul_at = dumped.find("Binary *").unwrap();
    assert!(mul_at > add_at);
}

#[test]
fn postfix_chains() {
    // a.b[0](c) — attr, then index, then call.
    let script = parse_ok("bark a.b[0](c)\nwow\n");
    match &script.stmts[0] {
        Stmt::Bark { expr, .. } => assert!(matches!(expr, Expr::Call { .. })),
        other => panic!("expected Bark, got {other:?}"),
    }
}

#[test]
fn multi_line_list_inside_brackets() {
    let script = parse_ok("such xs = [\n    1,\n    2,\n]\nwow\n");
    match &script.stmts[0] {
        Stmt::Decl { expr, .. } => match expr {
            Expr::List { items, .. } => assert_eq!(items.len(), 2),
            other => panic!("expected List, got {other:?}"),
        },
        other => panic!("expected Decl, got {other:?}"),
    }
}

#[test]
fn assign_to_non_target_is_an_error() {
    let err = parse_err("1 = 2\nwow\n");
    assert!(err.message.contains("cannot assign"));
}

#[test]
fn dump_matches_expected() {
    let script = parse_ok("such age = 7\nbark \"age is \" + age\nwow\n");
    let expected = "\
Script
  Decl age
    Int 7
  Bark
    Binary +
      Str \"age is \"
      Ident age
";
    assert_eq!(dump(&script), expected);
}
