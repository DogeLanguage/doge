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
                name: "age".into(),
                expr: Expr::Int {
                    value: 7,
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
fn hoisted_names_are_first_seen_order_and_unique() {
    // such a = 1 / for b in a: such a = 2 / pls: … oh no err! …
    let script = Script {
        stmts: vec![
            Stmt::Decl {
                name: "a".into(),
                expr: Expr::Int {
                    value: 1,
                    span: span(),
                },
                span: span(),
            },
            Stmt::For {
                var: "b".into(),
                iter: Expr::Ident {
                    name: "a".into(),
                    span: span(),
                },
                body: vec![Stmt::Decl {
                    name: "a".into(),
                    expr: Expr::Int {
                        value: 2,
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
        params: vec![],
        body: vec![Stmt::Decl {
            name: "hidden".into(),
            expr: Expr::Int {
                value: 0,
                span: span(),
            },
            span: span(),
        }],
        span: span(),
    };
    let loop_stmt = Stmt::For {
        var: "i".into(),
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
