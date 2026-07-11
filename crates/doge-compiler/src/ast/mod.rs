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

/// One formal parameter in a function header: its name and, optionally, a
/// default value. A default is a literal (docs/SYNTAX.md §6), so it references no
/// names and is safe to evaluate fresh at every call site.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub default: Option<Expr>,
    pub span: Span,
}

/// A function header's parameters: the fixed/defaulted positional parameters in
/// order, plus an optional trailing variadic parameter (`many rest`) that
/// collects the surplus arguments into a List. Required parameters come first,
/// then defaulted ones, then the variadic — the parser enforces that order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Params {
    pub params: Vec<Param>,
    pub vararg: Option<String>,
}

impl Params {
    /// True when the header declares no parameters at all.
    pub fn is_empty(&self) -> bool {
        self.params.is_empty() && self.vararg.is_none()
    }

    /// Whether a trailing `many rest` variadic parameter is present.
    pub fn has_vararg(&self) -> bool {
        self.vararg.is_some()
    }

    /// The number of leading parameters with no default — the minimum a call must
    /// supply.
    pub fn required(&self) -> usize {
        self.params.iter().filter(|p| p.default.is_none()).count()
    }

    /// The largest number of positional arguments the header can bind without a
    /// variadic parameter: every named parameter. `None` when a variadic is
    /// present (unbounded).
    pub fn max_positional(&self) -> Option<usize> {
        if self.has_vararg() {
            None
        } else {
            Some(self.params.len())
        }
    }

    /// The names the body binds: every parameter, then the variadic (which
    /// arrives as an already-packed List). This is what the compiled wrapper
    /// takes as value parameters.
    pub fn binding_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.params.iter().map(|p| p.name.clone()).collect();
        if let Some(rest) = &self.vararg {
            names.push(rest.clone());
        }
        names
    }

    /// The call shape shown in an arity hint, e.g. `greet(name, mood = …, many rest)`.
    pub fn render(&self, callee: &str) -> String {
        let mut parts: Vec<String> = self
            .params
            .iter()
            .map(|p| {
                if p.default.is_some() {
                    format!("{} = …", p.name)
                } else {
                    p.name.clone()
                }
            })
            .collect();
        if let Some(rest) = &self.vararg {
            parts.push(format!("many {rest}"));
        }
        format!("{callee}({})", parts.join(", "))
    }
}

/// One statement. Variants mirror the docs/GRAMMAR.md grammar.
// `ExprStmt` is the conventional AST name for a bare-expression statement; the
// suffix intentionally echoes the enum name, so silence that one style lint.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `such x = e`, or a destructuring `such a, b = e` / `such a, many rest = e`.
    /// `names` are the leading targets in order; `rest`, when present, is a
    /// trailing `many` collector that gathers the surplus into a List. A plain
    /// single declaration has one name and no `rest`.
    Decl {
        names: Vec<String>,
        rest: Option<String>,
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
    /// `[very] target = e`, or a destructuring `[very] a, b = e` /
    /// `[very] a, many rest = e` — `flavored` is true when written with `very`.
    /// `targets` are the leading assignable targets in order; `rest`, when
    /// present, is a trailing `many` collector target that gathers the surplus
    /// into a List. `op` is `Some` for an augmented assignment (`target op= e`),
    /// which reads the target, applies the binary operator, and stores the result
    /// back — augmented assignment is single-target only (one `targets` entry, no
    /// `rest`).
    Assign {
        targets: Vec<Expr>,
        rest: Option<Expr>,
        expr: Expr,
        op: Option<BinOp>,
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
    /// `for v in iter:`, or a destructuring `for k, v in iter:` /
    /// `for first, many rest in iter:`. `vars` are the leading loop variables in
    /// order; `rest`, when present, is a trailing `many` collector that gathers
    /// each element's surplus into a List. A plain loop has one var and no `rest`.
    For {
        vars: Vec<String>,
        rest: Option<String>,
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
        params: Params,
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
    /// `f(a, b, key = c)` — positional `args`, then any keyword arguments as
    /// `(name, value)` pairs in source order. Keyword arguments are only accepted
    /// where the callee is known at compile time (docs/SYNTAX.md §6).
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        kwargs: Vec<(String, Expr)>,
        span: Span,
    },
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// `obj[start:end:step]` — each bound is optional (`None` means the default
    /// end of that side), matching Python slice semantics.
    Slice {
        obj: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
        span: Span,
    },
    /// `then if cond else otherwise` — only the taken branch is evaluated.
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
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
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    In,
    NotIn,
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
            BinOp::Pow => "**",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Eq => "==",
            BinOp::NotEq => "!=",
            BinOp::Lt => "<",
            BinOp::LtEq => "<=",
            BinOp::Gt => ">",
            BinOp::GtEq => ">=",
            BinOp::In => "in",
            BinOp::NotIn => "not in",
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
    BitNot,
}

impl UnOp {
    pub fn symbol(self) -> &'static str {
        match self {
            UnOp::Not => "not",
            UnOp::Neg => "neg",
            UnOp::BitNot => "~",
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
            | Expr::Slice { span, .. }
            | Expr::Ternary { span, .. }
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
            Stmt::ConstDecl { name, .. } | Stmt::FuncDef { name, .. } => push_unique(names, name),
            Stmt::Decl {
                names: decl_names,
                rest,
                ..
            } => {
                for name in decl_names {
                    push_unique(names, name);
                }
                if let Some(rest) = rest {
                    push_unique(names, rest);
                }
            }
            Stmt::For { vars, rest, .. } => {
                for var in vars {
                    push_unique(names, var);
                }
                if let Some(rest) = rest {
                    push_unique(names, rest);
                }
            }
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
