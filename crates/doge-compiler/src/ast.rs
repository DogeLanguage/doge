use crate::token::Span;

/// A whole parsed script: a sequence of top-level statements (the terminating
/// `wow` is consumed by the parser and not stored).
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub stmts: Vec<Stmt>,
}

/// One statement. Variants mirror the DESIGN §5 grammar.
// `ExprStmt` is the conventional AST name for a bare-expression statement; the
// suffix intentionally echoes the enum name, so silence that one style lint.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `such x = e`
    Decl {
        name: String,
        expr: Expr,
        span: Span,
    },
    /// `so X = e`
    ConstDecl {
        name: String,
        expr: Expr,
        span: Span,
    },
    /// `so math`
    Import { module: String, span: Span },
    /// `[very] target = e` — `flavored` is true when written with `very`.
    Assign {
        target: Expr,
        expr: Expr,
        flavored: bool,
        span: Span,
    },
    /// `bark e`
    Bark { expr: Expr, span: Span },
    /// `if / elif / else`. Each branch is (condition, body); `else_body` is the
    /// optional trailing `else` block.
    If {
        branches: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
        span: Span,
    },
    /// `for v in iter:`
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `while cond:`
    While {
        cond: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `such name much params: … wow`
    FuncDef {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `many Name: … wow` — a body of function definitions (methods).
    ObjDef {
        name: String,
        methods: Vec<Stmt>,
        span: Span,
    },
    /// `pls … oh no err! …`
    Try {
        body: Vec<Stmt>,
        err_name: String,
        handler: Vec<Stmt>,
        span: Span,
    },
    /// `return [e]`
    Return { expr: Option<Expr>, span: Span },
    /// `bonk e` — raise a catchable error whose message is `e`'s display form.
    Bonk { expr: Expr, span: Span },
    /// `bork`
    Bork { span: Span },
    /// `continue`
    Continue { span: Span },
    /// A bare expression used as a statement, e.g. a call `greet(x)`.
    ExprStmt { expr: Expr },
}

/// One expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int {
        value: i64,
        span: Span,
    },
    Float {
        value: f64,
        span: Span,
    },
    Str {
        value: String,
        span: Span,
    },
    Bool {
        value: bool,
        span: Span,
    },
    None {
        span: Span,
    },
    Ident {
        name: String,
        span: Span,
    },
    List {
        items: Vec<Expr>,
        span: Span,
    },
    Dict {
        entries: Vec<(Expr, Expr)>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Attr {
        obj: Box<Expr>,
        name: String,
        span: Span,
    },
}

/// Binary operators, in the spelling they print with in the dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Rem,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::FloorDiv => "//",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::NotEq => "!=",
            BinOp::Lt => "<",
            BinOp::LtEq => "<=",
            BinOp::Gt => ">",
            BinOp::GtEq => ">=",
            BinOp::And => "and",
            BinOp::Or => "or",
        }
    }
}

/// Unary operators: `not` (logical) and `neg` (numeric minus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

impl UnOp {
    pub fn symbol(self) -> &'static str {
        match self {
            UnOp::Not => "not",
            UnOp::Neg => "neg",
        }
    }
}

impl Expr {
    /// The source span this expression starts at.
    pub fn span(&self) -> Span {
        match self {
            Expr::Int { span, .. }
            | Expr::Float { span, .. }
            | Expr::Str { span, .. }
            | Expr::Bool { span, .. }
            | Expr::None { span }
            | Expr::Ident { span, .. }
            | Expr::List { span, .. }
            | Expr::Dict { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Call { span, .. }
            | Expr::Index { span, .. }
            | Expr::Attr { span, .. } => *span,
        }
    }
}

/// Render a script as an indented tree (two spaces per level). Stable and
/// language-agnostic — this is what `doge check` prints on success.
pub fn dump(script: &Script) -> String {
    let mut out = String::new();
    out.push_str("Script\n");
    for stmt in &script.stmts {
        dump_stmt(stmt, 1, &mut out);
    }
    out
}

fn indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn line(level: usize, text: &str, out: &mut String) {
    indent(level, out);
    out.push_str(text);
    out.push('\n');
}

fn dump_block(label: &str, body: &[Stmt], level: usize, out: &mut String) {
    line(level, label, out);
    for stmt in body {
        dump_stmt(stmt, level + 1, out);
    }
}

fn dump_stmt(stmt: &Stmt, level: usize, out: &mut String) {
    match stmt {
        Stmt::Decl { name, expr, .. } => {
            line(level, &format!("Decl {name}"), out);
            dump_expr(expr, level + 1, out);
        }
        Stmt::ConstDecl { name, expr, .. } => {
            line(level, &format!("ConstDecl {name}"), out);
            dump_expr(expr, level + 1, out);
        }
        Stmt::Import { module, .. } => line(level, &format!("Import {module}"), out),
        Stmt::Assign {
            target,
            expr,
            flavored,
            ..
        } => {
            let label = if *flavored { "Assign very" } else { "Assign" };
            line(level, label, out);
            dump_block_expr("target", target, level + 1, out);
            dump_block_expr("value", expr, level + 1, out);
        }
        Stmt::Bark { expr, .. } => {
            line(level, "Bark", out);
            dump_expr(expr, level + 1, out);
        }
        Stmt::If {
            branches,
            else_body,
            ..
        } => {
            line(level, "If", out);
            for (cond, body) in branches {
                line(level + 1, "branch", out);
                dump_block_expr("cond", cond, level + 2, out);
                dump_block("body", body, level + 2, out);
            }
            if let Some(body) = else_body {
                dump_block("else", body, level + 1, out);
            }
        }
        Stmt::For {
            var, iter, body, ..
        } => {
            line(level, &format!("For {var}"), out);
            dump_block_expr("in", iter, level + 1, out);
            dump_block("body", body, level + 1, out);
        }
        Stmt::While { cond, body, .. } => {
            line(level, "While", out);
            dump_block_expr("cond", cond, level + 1, out);
            dump_block("body", body, level + 1, out);
        }
        Stmt::FuncDef {
            name, params, body, ..
        } => {
            let params = if params.is_empty() {
                String::new()
            } else {
                format!(" much {}", params.join(", "))
            };
            dump_block(&format!("FuncDef {name}{params}"), body, level, out);
        }
        Stmt::ObjDef { name, methods, .. } => {
            dump_block(&format!("ObjDef {name}"), methods, level, out);
        }
        Stmt::Try {
            body,
            err_name,
            handler,
            ..
        } => {
            line(level, "Try", out);
            dump_block("body", body, level + 1, out);
            dump_block(&format!("catch {err_name}"), handler, level + 1, out);
        }
        Stmt::Return { expr, .. } => match expr {
            Some(expr) => {
                line(level, "Return", out);
                dump_expr(expr, level + 1, out);
            }
            None => line(level, "Return", out),
        },
        Stmt::Bonk { expr, .. } => {
            line(level, "Bonk", out);
            dump_expr(expr, level + 1, out);
        }
        Stmt::Bork { .. } => line(level, "Bork", out),
        Stmt::Continue { .. } => line(level, "Continue", out),
        Stmt::ExprStmt { expr } => {
            line(level, "ExprStmt", out);
            dump_expr(expr, level + 1, out);
        }
    }
}

/// Dump an expression under a named sub-heading, e.g. `cond` / `target`.
fn dump_block_expr(label: &str, expr: &Expr, level: usize, out: &mut String) {
    line(level, label, out);
    dump_expr(expr, level + 1, out);
}

fn dump_expr(expr: &Expr, level: usize, out: &mut String) {
    match expr {
        Expr::Int { value, .. } => line(level, &format!("Int {value}"), out),
        Expr::Float { value, .. } => line(level, &format!("Float {value}"), out),
        Expr::Str { value, .. } => line(level, &format!("Str {value:?}"), out),
        Expr::Bool { value, .. } => line(level, &format!("Bool {value}"), out),
        Expr::None { .. } => line(level, "None", out),
        Expr::Ident { name, .. } => line(level, &format!("Ident {name}"), out),
        Expr::List { items, .. } => {
            line(level, "List", out);
            for item in items {
                dump_expr(item, level + 1, out);
            }
        }
        Expr::Dict { entries, .. } => {
            line(level, "Dict", out);
            for (key, value) in entries {
                line(level + 1, "entry", out);
                dump_block_expr("key", key, level + 2, out);
                dump_block_expr("value", value, level + 2, out);
            }
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            line(level, &format!("Binary {}", op.symbol()), out);
            dump_expr(lhs, level + 1, out);
            dump_expr(rhs, level + 1, out);
        }
        Expr::Unary { op, operand, .. } => {
            line(level, &format!("Unary {}", op.symbol()), out);
            dump_expr(operand, level + 1, out);
        }
        Expr::Call { callee, args, .. } => {
            line(level, "Call", out);
            dump_block_expr("callee", callee, level + 1, out);
            for arg in args {
                dump_block_expr("arg", arg, level + 1, out);
            }
        }
        Expr::Index { obj, index, .. } => {
            line(level, "Index", out);
            dump_block_expr("obj", obj, level + 1, out);
            dump_block_expr("index", index, level + 1, out);
        }
        Expr::Attr { obj, name, .. } => {
            line(level, &format!("Attr {name}"), out);
            dump_expr(obj, level + 1, out);
        }
    }
}

#[cfg(test)]
mod tests {
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
}
