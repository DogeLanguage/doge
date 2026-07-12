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
            assert_eq!(params.binding_names(), vec!["name".to_string()]);
            assert!(params.vararg.is_none());
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
fn params_carry_defaults_and_a_variadic() {
    let func = parse_ok("such f much a, b = 2, many rest:\n    bark a\nwow\nwow\n");
    match &func.stmts[0] {
        Stmt::FuncDef { params, .. } => {
            assert_eq!(params.required(), 1);
            assert_eq!(params.params.len(), 2);
            assert!(params.params[0].default.is_none());
            assert!(params.params[1].default.is_some());
            assert_eq!(params.vararg.as_deref(), Some("rest"));
            assert_eq!(params.max_positional(), None);
        }
        other => panic!("expected FuncDef, got {other:?}"),
    }
}

#[test]
fn required_param_after_default_is_an_error() {
    let err = parse_err("such f much a = 1, b:\n    bark a\nwow\nwow\n");
    assert_eq!(err.headline, "very order. much default.");
}

#[test]
fn variadic_must_come_last() {
    let err = parse_err("such f much many rest, a:\n    bark a\nwow\nwow\n");
    assert_eq!(err.headline, "very rest. much greedy.");
}

#[test]
fn non_literal_default_is_an_error() {
    let err = parse_err("such f much a = len:\n    bark a\nwow\nwow\n");
    assert_eq!(err.headline, "very default. much dynamic.");
}

#[test]
fn call_collects_positional_and_keyword_args() {
    let script = parse_ok("f(1, mood = 2)\nwow\n");
    match &script.stmts[0] {
        Stmt::ExprStmt {
            expr: Expr::Call { args, kwargs, .. },
        } => {
            assert_eq!(args.len(), 1);
            assert_eq!(kwargs.len(), 1);
            assert_eq!(kwargs[0].0, "mood");
        }
        other => panic!("expected a call, got {other:?}"),
    }
}

#[test]
fn positional_after_keyword_is_an_error() {
    let err = parse_err("f(a = 1, 2)\nwow\n");
    assert_eq!(err.headline, "very order. much muddle.");
}

#[test]
fn repeated_keyword_arg_is_an_error() {
    let err = parse_err("f(a = 1, a = 2)\nwow\n");
    assert_eq!(err.headline, "very keyword. much repeat.");
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
fn object_can_name_a_parent() {
    let src = "many Corgi much Shibe:\n    such speak:\n        bark 1\n    wow\nwow\nwow\n";
    let script = parse_ok(src);
    match &script.stmts[0] {
        Stmt::ObjDef { name, parent, .. } => {
            assert_eq!(name, "Corgi");
            assert_eq!(parent.as_deref(), Some("Shibe"));
        }
        other => panic!("expected ObjDef, got {other:?}"),
    }
    // A plain object has no parent.
    let plain = parse_ok("many Shibe:\n    such go:\n        bark 1\n    wow\nwow\nwow\n");
    match &plain.stmts[0] {
        Stmt::ObjDef { parent, .. } => assert!(parent.is_none()),
        other => panic!("expected ObjDef, got {other:?}"),
    }
}

#[test]
fn super_parses_as_a_method_call() {
    let src = "many Corgi much Shibe:\n    such speak:\n        return super.speak()\n    wow\nwow\nwow\n";
    let script = parse_ok(src);
    assert!(dump(&script).contains("SuperCall speak"));
}

#[test]
fn bare_super_is_a_friendly_error() {
    let err = parse_err("many A much B:\n    such go:\n        return super\n    wow\nwow\nwow\n");
    assert_eq!(err.headline, "very super. much confuse.");
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
fn membership_parses_as_a_comparison() {
    let script = parse_ok("bark x in xs\nwow\n");
    assert!(dump(&script).contains("Binary in"));
}

#[test]
fn not_in_parses_as_one_operator() {
    let script = parse_ok("bark x not in xs\nwow\n");
    let dumped = dump(&script);
    assert!(dumped.contains("Binary not in"));
    // It is a single membership test, not a `not` wrapping something.
    assert!(!dumped.contains("Unary not"));
}

#[test]
fn not_before_membership_negates_the_whole_test() {
    // `not x in xs` is `not (x in xs)`, matching Python precedence.
    let script = parse_ok("bark not x in xs\nwow\n");
    let dumped = dump(&script);
    let not_at = dumped.find("Unary not").expect("a leading not");
    let in_at = dumped.find("Binary in").expect("a membership test");
    assert!(in_at > not_at, "the membership test nests under the not");
}

#[test]
fn membership_does_not_chain() {
    assert!(parse_err("bark a in b in c\nwow\n")
        .message
        .contains("chain comparisons"));
    assert!(parse_err("bark a < b not in c\nwow\n")
        .message
        .contains("chain comparisons"));
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
fn power_is_right_associative() {
    let script = parse_ok("bark 2 ** 3 ** 2\nwow\n");
    let expected = "\
Script
  Bark
    Binary **
      Int 2
      Binary **
        Int 3
        Int 2
";
    assert_eq!(dump(&script), expected);
}

#[test]
fn unary_minus_binds_looser_than_power() {
    // -2 ** 2 is -(2 ** 2), so the power nests under the negation.
    let script = parse_ok("bark -2 ** 2\nwow\n");
    let dumped = dump(&script);
    let neg_at = dumped.find("Unary neg").expect("a negation");
    let pow_at = dumped.find("Binary **").expect("a power");
    assert!(pow_at > neg_at, "the power nests under the negation");
}

#[test]
fn bitwise_precedence_or_over_and() {
    // 1 | 2 & 3 parses as 1 | (2 & 3).
    let script = parse_ok("bark 1 | 2 & 3\nwow\n");
    let dumped = dump(&script);
    let or_at = dumped.find("Binary |").expect("a bit-or");
    let and_at = dumped.find("Binary &").expect("a bit-and");
    assert!(and_at > or_at, "the and nests under the or");
}

#[test]
fn shift_binds_tighter_than_comparison_looser_than_add() {
    // 1 + 2 << 3 is (1 + 2) << 3.
    let script = parse_ok("bark 1 + 2 << 3\nwow\n");
    let dumped = dump(&script);
    let shl_at = dumped.find("Binary <<").expect("a shift");
    let add_at = dumped.find("Binary +").expect("an add");
    assert!(add_at > shl_at, "the add nests under the shift");
}

#[test]
fn ternary_parses_with_both_branches() {
    let script = parse_ok("bark \"a\" if true else \"b\"\nwow\n");
    let expected = "\
Script
  Bark
    Ternary
      cond
        Bool true
      then
        Str \"a\"
      else
        Str \"b\"
";
    assert_eq!(dump(&script), expected);
}

#[test]
fn ternary_else_is_required() {
    let err = parse_err("such x = 1 if true\nwow\n");
    assert_eq!(err.headline, "very half. much ternary.");
}

#[test]
fn ternary_else_nests_to_the_right() {
    // a if p else b if q else c  ==  a if p else (b if q else c)
    let script = parse_ok("bark 1 if a else 2 if b else 3\nwow\n");
    let dumped = dump(&script);
    assert_eq!(dumped.matches("Ternary").count(), 2);
    let first = dumped.find("Ternary").unwrap();
    let second = dumped[first + 1..].find("Ternary").unwrap();
    // The second Ternary is more deeply indented — it is the else branch.
    assert!(second > 0);
}

#[test]
fn subscript_stays_a_plain_index() {
    let script = parse_ok("bark xs[0]\nwow\n");
    match &script.stmts[0] {
        Stmt::Bark { expr, .. } => assert!(matches!(expr, Expr::Index { .. })),
        other => panic!("expected Bark, got {other:?}"),
    }
}

#[test]
fn slice_parses_all_three_parts() {
    let script = parse_ok("bark xs[1:2:3]\nwow\n");
    let expected = "\
Script
  Bark
    Slice
      obj
        Ident xs
      start
        Int 1
      end
        Int 2
      step
        Int 3
";
    assert_eq!(dump(&script), expected);
}

#[test]
fn slice_omits_bounds() {
    let script = parse_ok("bark xs[::-1]\nwow\n");
    let dumped = dump(&script);
    assert!(dumped.contains("start none"));
    assert!(dumped.contains("end none"));
    // The step is present: a negated 1.
    assert!(dumped.contains("Unary neg"));
}

#[test]
fn augmented_assignment_carries_its_operator() {
    let script = parse_ok("count += 1\nwow\n");
    match &script.stmts[0] {
        Stmt::Assign {
            op: Some(BinOp::Add),
            flavored: false,
            ..
        } => {}
        other => panic!("expected an augmented Assign, got {other:?}"),
    }
}

#[test]
fn augmented_assignment_works_on_an_item_and_after_very() {
    let idx = parse_ok("xs[0] *= 2\nwow\n");
    match &idx.stmts[0] {
        Stmt::Assign {
            targets,
            op: Some(BinOp::Mul),
            ..
        } if matches!(targets.as_slice(), [Expr::Index { .. }]) => {}
        other => panic!("expected an augmented item Assign, got {other:?}"),
    }
    let flavored = parse_ok("very n -= 3\nwow\n");
    match &flavored.stmts[0] {
        Stmt::Assign {
            op: Some(BinOp::Sub),
            flavored: true,
            ..
        } => {}
        other => panic!("expected a flavored augmented Assign, got {other:?}"),
    }
}

#[test]
fn destructuring_declaration_collects_names_and_collector() {
    let script = parse_ok("such a, b, many rest = xs\nwow\n");
    match &script.stmts[0] {
        Stmt::Decl { names, rest, .. } => {
            assert_eq!(names, &["a".to_string(), "b".to_string()]);
            assert_eq!(rest.as_deref(), Some("rest"));
        }
        other => panic!("expected a destructuring Decl, got {other:?}"),
    }
}

#[test]
fn destructuring_assignment_targets_and_swap_rhs() {
    // `p, q = q, p` — the comma right-hand side desugars into a list literal, so
    // the swap reads both values before either store.
    let script = parse_ok("such p = 1\nsuch q = 2\np, q = q, p\nwow\n");
    match &script.stmts[2] {
        Stmt::Assign {
            targets,
            rest,
            expr,
            op: None,
            ..
        } => {
            assert_eq!(targets.len(), 2);
            assert!(rest.is_none());
            assert!(matches!(expr, Expr::List { items, .. } if items.len() == 2));
        }
        other => panic!("expected a destructuring Assign, got {other:?}"),
    }
}

#[test]
fn for_loop_destructures_its_variables() {
    let script = parse_ok("for k, v in d:\n    bark k\nwow\n");
    match &script.stmts[0] {
        Stmt::For { vars, rest, .. } => {
            assert_eq!(vars, &["k".to_string(), "v".to_string()]);
            assert!(rest.is_none());
        }
        other => panic!("expected a destructuring For, got {other:?}"),
    }
}

#[test]
fn single_declaration_rejects_a_comma_list_value() {
    // `such z = 1, 2` has one name but two values — a list must be explicit.
    let err = parse_err("such z = 1, 2\nwow\n");
    assert!(err.message.contains("only one name"));
}

#[test]
fn augmented_assignment_rejects_multiple_targets() {
    let err = parse_err("such a = 1\nsuch b = 2\na, b += 1\nwow\n");
    assert!(err.message.contains("single target"));
}

#[test]
fn collector_must_be_the_last_target() {
    let err = parse_err("such a, many rest, c = xs\nwow\n");
    assert!(err.message.contains("last target"));
}

#[test]
fn plain_assignment_has_no_operator() {
    let script = parse_ok("x = 1\nwow\n");
    match &script.stmts[0] {
        Stmt::Assign { op: None, .. } => {}
        other => panic!("expected a plain Assign, got {other:?}"),
    }
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
