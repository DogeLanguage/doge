use super::*;

fn span() -> Span {
    Span { line: 1, col: 1 }
}

#[test]
fn dump_pins_the_tree_shape() {
    // such age = 7
    // bark "age is " + age
    let script = Script {
        stmts: vec![
            Stmt::Decl {
                names: vec!["age".into()],
                rest: None,
                expr: Expr::Int {
                    value: num_bigint::BigInt::from(7),
                    span: span(),
                },
                span: span(),
            },
            Stmt::Bark {
                expr: Expr::Binary {
                    op: BinOp::Add,
                    lhs: Box::new(Expr::Str {
                        value: "age is ".into(),
                        span: span(),
                    }),
                    rhs: Box::new(Expr::Ident {
                        name: "age".into(),
                        span: span(),
                    }),
                    span: span(),
                },
                span: span(),
            },
        ],
    };

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

#[test]
fn dump_pins_the_destructuring_shape() {
    // such a, many rest = xs
    // p, q = q, p        (parser desugars the comma-RHS into a list)
    let script = Script {
        stmts: vec![
            Stmt::Decl {
                names: vec!["a".into()],
                rest: Some("rest".into()),
                expr: Expr::Ident {
                    name: "xs".into(),
                    span: span(),
                },
                span: span(),
            },
            Stmt::Assign {
                targets: vec![
                    Expr::Ident {
                        name: "p".into(),
                        span: span(),
                    },
                    Expr::Ident {
                        name: "q".into(),
                        span: span(),
                    },
                ],
                rest: None,
                expr: Expr::List {
                    items: vec![
                        Expr::Ident {
                            name: "q".into(),
                            span: span(),
                        },
                        Expr::Ident {
                            name: "p".into(),
                            span: span(),
                        },
                    ],
                    span: span(),
                },
                op: None,
                flavored: false,
                span: span(),
            },
        ],
    };

    let expected = "\
Script
  Decl a, many rest
    Ident xs
  Assign
    target
      Ident p
    target
      Ident q
    value
      List
        Ident q
        Ident p
";
    assert_eq!(dump(&script), expected);
}

#[test]
fn hoisted_names_include_destructuring_targets_and_collectors() {
    // such a, b = xs / for k, many rest in ys: …
    let script = Script {
        stmts: vec![
            Stmt::Decl {
                names: vec!["a".into(), "b".into()],
                rest: None,
                expr: Expr::Ident {
                    name: "xs".into(),
                    span: span(),
                },
                span: span(),
            },
            Stmt::For {
                vars: vec!["k".into()],
                rest: Some("rest".into()),
                iter: Expr::Ident {
                    name: "ys".into(),
                    span: span(),
                },
                body: vec![],
                span: span(),
            },
        ],
    };

    assert_eq!(hoisted_names(&script.stmts), vec!["a", "b", "k", "rest"]);
}

#[test]
fn hoisted_names_are_first_seen_order_and_unique() {
    // such a = 1 / for b in a: such a = 2 / pls: … oh no err! …
    let script = Script {
        stmts: vec![
            Stmt::Decl {
                names: vec!["a".into()],
                rest: None,
                expr: Expr::Int {
                    value: num_bigint::BigInt::from(1),
                    span: span(),
                },
                span: span(),
            },
            Stmt::For {
                vars: vec!["b".into()],
                rest: None,
                iter: Expr::Ident {
                    name: "a".into(),
                    span: span(),
                },
                body: vec![Stmt::Decl {
                    names: vec!["a".into()],
                    rest: None,
                    expr: Expr::Int {
                        value: num_bigint::BigInt::from(2),
                        span: span(),
                    },
                    span: span(),
                }],
                span: span(),
            },
            Stmt::Try {
                body: vec![],
                err_name: "err".into(),
                handler: vec![],
                span: span(),
            },
        ],
    };

    assert_eq!(hoisted_names(&script.stmts), vec!["a", "b", "err"]);
}

#[test]
fn for_each_child_block_skips_nested_function_bodies() {
    // A function nested in a for-loop body: the walker visits the loop body but
    // not the function's own body.
    let funcdef = Stmt::FuncDef {
        name: "inner".into(),
        params: Params::default(),
        body: vec![Stmt::Decl {
            names: vec!["hidden".into()],
            rest: None,
            expr: Expr::Int {
                value: num_bigint::BigInt::from(0),
                span: span(),
            },
            span: span(),
        }],
        span: span(),
    };
    let loop_stmt = Stmt::For {
        vars: vec!["i".into()],
        rest: None,
        iter: Expr::List {
            items: vec![],
            span: span(),
        },
        body: vec![funcdef.clone()],
        span: span(),
    };

    let mut blocks = 0;
    for_each_child_block(&loop_stmt, &mut |body| {
        blocks += 1;
        assert_eq!(body.len(), 1);
    });
    assert_eq!(blocks, 1);

    // Visiting the funcdef itself yields no child blocks in this scope.
    let mut inner_blocks = 0;
    for_each_child_block(&funcdef, &mut |_| inner_blocks += 1);
    assert_eq!(inner_blocks, 0);
}
