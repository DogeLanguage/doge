use crate::token::Span;

mod dump;
#[cfg(test)]
mod tests;

pub use dump::dump;

/// A whole parsed script: a sequence of top-level statements (the terminating
/// `wow` is consumed by the parser and not stored).
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub stmts: Vec<Stmt>,
}

/// One statement. Variants mirror the docs/GRAMMAR.md grammar.
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
    StrInterp {
        parts: Vec<InterpPart>,
        span: Span,
    },
}

/// One piece of a string-interpolation expression (`"a {b} c"`): literal text or
/// an embedded expression whose display form is spliced in at runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpPart {
    Lit(String),
    Expr(Expr),
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

impl Stmt {
    /// The source span this statement starts at.
    pub fn span(&self) -> Span {
        match self {
            Stmt::Decl { span, .. }
            | Stmt::ConstDecl { span, .. }
            | Stmt::Import { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::Bark { span, .. }
            | Stmt::If { span, .. }
            | Stmt::For { span, .. }
            | Stmt::While { span, .. }
            | Stmt::FuncDef { span, .. }
            | Stmt::ObjDef { span, .. }
            | Stmt::Try { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Bonk { span, .. }
            | Stmt::Bork { span }
            | Stmt::Continue { span } => *span,
            Stmt::ExprStmt { expr } => expr.span(),
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
            | Expr::Attr { span, .. }
            | Expr::StrInterp { span, .. } => *span,
        }
    }
}

/// Call `f` with each statement block a statement contributes to its *enclosing*
/// scope: an `if`'s branch and `else` bodies, a `for`/`while` body, and a
/// `pls`/`oh no` body and handler. A nested function's or object's own body is
/// deliberately not visited — those statements belong to their own scope, not
/// this one. This match is exhaustive with no wildcard, so a new block-carrying
/// statement cannot be silently skipped by the scope collectors and capture
/// analysis that fold over it.
pub(crate) fn for_each_child_block<'a>(stmt: &'a Stmt, f: &mut impl FnMut(&'a [Stmt])) {
    match stmt {
        Stmt::If {
            branches,
            else_body,
            ..
        } => {
            for (_, body) in branches {
                f(body);
            }
            if let Some(body) = else_body {
                f(body);
            }
        }
        Stmt::For { body, .. } | Stmt::While { body, .. } => f(body),
        Stmt::Try { body, handler, .. } => {
            f(body);
            f(handler);
        }
        Stmt::Decl { .. }
        | Stmt::ConstDecl { .. }
        | Stmt::Import { .. }
        | Stmt::Assign { .. }
        | Stmt::Bark { .. }
        | Stmt::FuncDef { .. }
        | Stmt::ObjDef { .. }
        | Stmt::Return { .. }
        | Stmt::Bonk { .. }
        | Stmt::Bork { .. }
        | Stmt::Continue { .. }
        | Stmt::ExprStmt { .. } => {}
    }
}

/// Every bound name in one scope — `such`/`so` declarations, `for` loop
/// variables, `oh no` error names, and nested function definitions — in
/// first-seen order, each once. These become the scope's `Env` fields or hoisted
/// locals. A nested function's own body is not descended into: its inner names
/// belong to its own scope.
pub(crate) fn hoisted_names(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    collect_hoisted(stmts, &mut names);
    names
}

/// The top-level hoisted names for `Env` fields: like [`hoisted_names`] but a
/// direct top-level `such name:` / `many Name:` is a static definition, not an
/// `Env` field, so those direct definitions are skipped (a function nested inside
/// a top-level `if`/`for` block is still a closure and does get a field).
pub(crate) fn toplevel_hoisted(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in stmts {
        if matches!(stmt, Stmt::FuncDef { .. } | Stmt::ObjDef { .. }) {
            continue;
        }
        collect_hoisted(std::slice::from_ref(stmt), &mut names);
    }
    names
}

pub(crate) fn collect_hoisted(stmts: &[Stmt], names: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            // A nested function binds its name in this scope; its body is its own.
            Stmt::Decl { name, .. } | Stmt::ConstDecl { name, .. } | Stmt::FuncDef { name, .. } => {
                push_unique(names, name)
            }
            Stmt::For { var, .. } => push_unique(names, var),
            Stmt::Try { err_name, .. } => push_unique(names, err_name),
            _ => {}
        }
        for_each_child_block(stmt, &mut |body| collect_hoisted(body, names));
    }
}

pub(crate) fn push_unique(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
    }
}
