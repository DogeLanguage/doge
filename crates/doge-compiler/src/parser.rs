use crate::ast::{BinOp, Expr, Script, Stmt, UnOp};
use crate::diagnostics::Diagnostic;
use crate::lexer;
use crate::token::{Span, Token, TokenKind};

/// Lex then parse `source` (from `path`) into a [`Script`], or return the first
/// [`Diagnostic`].
pub fn parse(path: &str, source: &str) -> Result<Script, Diagnostic> {
    let tokens = lexer::lex(path, source)?;
    Parser::new(path, source, tokens).parse_script()
}

struct Parser {
    path: String,
    lines: Vec<String>,
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(path: &str, source: &str, tokens: Vec<Token>) -> Parser {
        let lines = source
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();
        Parser {
            path: path.to_string(),
            lines,
            tokens,
            pos: 0,
        }
    }

    // ----- token cursor helpers -----

    fn peek(&self) -> &TokenKind {
        // The lexer always terminates the stream with Eof, so this is in range.
        &self.tokens[self.pos].kind
    }

    fn current_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn is(&self, kind: &TokenKind) -> bool {
        self.peek() == kind
    }

    /// Consume a token of the given (payload-less) kind, or produce a
    /// "expected X but found Y" diagnostic.
    fn eat(&mut self, kind: TokenKind) -> Result<Token, Diagnostic> {
        if self.peek() == &kind {
            Ok(self.advance())
        } else {
            let span = self.current_span();
            Err(self.diag(
                span,
                format!(
                    "doge expected {} here, but found {}",
                    kind.describe(),
                    self.peek().describe()
                ),
            ))
        }
    }

    /// Consume an identifier and return its name, or a friendly diagnostic.
    fn eat_ident(&mut self, what: &str) -> Result<(String, Span), Diagnostic> {
        let span = self.current_span();
        if let TokenKind::Ident(name) = self.peek() {
            let name = name.clone();
            self.advance();
            Ok((name, span))
        } else {
            Err(self.diag(
                span,
                format!(
                    "doge expected {what} here, but found {}",
                    self.peek().describe()
                ),
            ))
        }
    }

    fn diag(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        let source_line = self
            .lines
            .get((span.line as usize).saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        Diagnostic::new(&self.path, span.line, span.col, source_line, message)
    }

    fn missing_wow(&self, span: Span, what: &str) -> Diagnostic {
        self.diag(span, format!("expected wow to close this {what}"))
            .with_headline("very incomplete. such missing wow.")
            .with_hint(
                "every function, object, and script ends with wow (did the script end early?)",
            )
    }

    // ----- top level -----

    fn parse_script(&mut self) -> Result<Script, Diagnostic> {
        let mut stmts = Vec::new();
        while !self.is(&TokenKind::Wow) {
            if self.is(&TokenKind::Eof) {
                return Err(self.missing_wow(self.current_span(), "script"));
            }
            stmts.push(self.parse_statement()?);
        }
        self.eat(TokenKind::Wow)?;
        // The lexer always follows the `wow` line with a Newline.
        if self.is(&TokenKind::Newline) {
            self.advance();
        }
        if !self.is(&TokenKind::Eof) {
            let span = self.current_span();
            return Err(self
                .diag(
                    span,
                    "doge stops reading at wow — nothing may come after it",
                )
                .with_headline("very extra. much after wow.")
                .with_hint("remove the lines after wow, or move them above it"));
        }
        Ok(Script { stmts })
    }

    // ----- statements -----

    fn parse_statement(&mut self) -> Result<Stmt, Diagnostic> {
        match self.peek() {
            TokenKind::Such => self.parse_such(),
            TokenKind::So => self.parse_so(),
            TokenKind::Many => self.parse_many(),
            TokenKind::Very => self.parse_very(),
            TokenKind::Pls => self.parse_pls(),
            TokenKind::If => self.parse_if(),
            TokenKind::For => self.parse_for(),
            TokenKind::While => self.parse_while(),
            TokenKind::Bark => self.parse_bark(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Bork => {
                let span = self.current_span();
                self.advance();
                self.eat(TokenKind::Newline)?;
                Ok(Stmt::Bork { span })
            }
            TokenKind::Continue => {
                let span = self.current_span();
                self.advance();
                self.eat(TokenKind::Newline)?;
                Ok(Stmt::Continue { span })
            }
            TokenKind::Def => {
                Err(self.python_habit("no def here", "such greet much name: is the way"))
            }
            TokenKind::Class => Err(self.python_habit("no class here", "many Name: is the way")),
            TokenKind::Amaze => {
                let span = self.current_span();
                Err(self
                    .diag(span, "amaze is reserved for a future doge")
                    .with_headline("very reserved. much later.")
                    .with_hint("pick another name for now"))
            }
            _ => self.parse_expr_or_assign(),
        }
    }

    fn python_habit(&self, message: &str, hint: &str) -> Diagnostic {
        let span = self.current_span();
        self.diag(span, message)
            .with_headline("very python. much habit.")
            .with_hint(hint)
    }

    /// `such` is contextual: `such NAME =` is a variable, `such NAME :` or
    /// `such NAME much …:` is a function definition (DESIGN §5).
    fn parse_such(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Such)?;
        let (name, _) = self.eat_ident("a name after such")?;

        match self.peek() {
            TokenKind::Eq => {
                self.advance();
                let expr = self.parse_expr()?;
                self.eat(TokenKind::Newline)?;
                Ok(Stmt::Decl { name, expr, span })
            }
            TokenKind::Much | TokenKind::Colon => {
                let mut params = Vec::new();
                if self.is(&TokenKind::Much) {
                    self.advance();
                    params = self.parse_params()?;
                }
                self.eat(TokenKind::Colon)?;
                let body = self.parse_block()?;
                if !self.is(&TokenKind::Wow) {
                    return Err(self.missing_wow(self.current_span(), "function"));
                }
                self.advance(); // wow
                self.eat(TokenKind::Newline)?;
                Ok(Stmt::FuncDef {
                    name,
                    params,
                    body,
                    span,
                })
            }
            _ => {
                let bad = self.current_span();
                Err(self
                    .diag(
                        bad,
                        format!("after such {name}, doge expects = (a variable) or : (a function)"),
                    )
                    .with_hint("such x = 1  —  or  —  such greet much name:"))
            }
        }
    }

    /// `so` is contextual: `so NAME =` is a constant, bare `so NAME` is an
    /// import (DESIGN §5).
    fn parse_so(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::So)?;
        let (name, _) = self.eat_ident("a name after so")?;
        match self.peek() {
            TokenKind::Eq => {
                self.advance();
                let expr = self.parse_expr()?;
                self.eat(TokenKind::Newline)?;
                Ok(Stmt::ConstDecl { name, expr, span })
            }
            TokenKind::Newline => {
                self.advance();
                Ok(Stmt::Import { module: name, span })
            }
            _ => {
                let bad = self.current_span();
                Err(self
                    .diag(
                        bad,
                        format!("after so {name}, doge expects = (a constant) or a line end (an import)"),
                    )
                    .with_hint("so PI = 3.14  —  or  —  so math"))
            }
        }
    }

    /// `many NAME: … wow` — an object definition whose body is only functions.
    fn parse_many(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Many)?;
        let (name, _) = self.eat_ident("an object name after many")?;
        self.eat(TokenKind::Colon)?;
        let body = self.parse_block()?;
        let mut methods = Vec::new();
        for stmt in body {
            match stmt {
                Stmt::FuncDef { .. } => methods.push(stmt),
                other => {
                    let bad = stmt_span(&other);
                    return Err(self
                        .diag(bad, "an object body may only hold methods (such …:)")
                        .with_headline("very object. much confuse.")
                        .with_hint("move this out of the object, or make it a method"));
                }
            }
        }
        if !self.is(&TokenKind::Wow) {
            return Err(self.missing_wow(self.current_span(), "object"));
        }
        self.advance(); // wow
        self.eat(TokenKind::Newline)?;
        Ok(Stmt::ObjDef {
            name,
            methods,
            span,
        })
    }

    fn parse_very(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Very)?;
        let target = self.parse_expr()?;
        self.eat(TokenKind::Eq).map_err(|_| {
            self.diag(
                target.span(),
                "very always reassigns — doge expects = after the target",
            )
            .with_hint("very age = 9")
        })?;
        let expr = self.parse_expr()?;
        self.eat(TokenKind::Newline)?;
        self.require_target(&target)?;
        Ok(Stmt::Assign {
            target,
            expr,
            flavored: true,
            span,
        })
    }

    fn parse_pls(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Pls)?;
        let body = self.parse_block()?;
        self.eat(TokenKind::OhNo)?;
        let (err_name, _) = self.eat_ident("the caught error's name after oh no")?;
        self.eat(TokenKind::Bang)?;
        let handler = self.parse_block()?;
        Ok(Stmt::Try {
            body,
            err_name,
            handler,
            span,
        })
    }

    fn parse_if(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::If)?;
        let mut branches = Vec::new();
        let cond = self.parse_expr()?;
        self.eat(TokenKind::Colon)?;
        let body = self.parse_block()?;
        branches.push((cond, body));

        while self.is(&TokenKind::Elif) {
            self.advance();
            let cond = self.parse_expr()?;
            self.eat(TokenKind::Colon)?;
            let body = self.parse_block()?;
            branches.push((cond, body));
        }

        let else_body = if self.is(&TokenKind::Else) {
            self.advance();
            self.eat(TokenKind::Colon)?;
            Some(self.parse_block()?)
        } else {
            None
        };

        Ok(Stmt::If {
            branches,
            else_body,
            span,
        })
    }

    fn parse_for(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::For)?;
        let (var, _) = self.eat_ident("a loop variable after for")?;
        self.eat(TokenKind::In)?;
        let iter = self.parse_expr()?;
        self.eat(TokenKind::Colon)?;
        let body = self.parse_block()?;
        Ok(Stmt::For {
            var,
            iter,
            body,
            span,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::While)?;
        let cond = self.parse_expr()?;
        self.eat(TokenKind::Colon)?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body, span })
    }

    fn parse_bark(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Bark)?;
        let expr = self.parse_expr()?;
        self.eat(TokenKind::Newline)?;
        Ok(Stmt::Bark { expr, span })
    }

    fn parse_return(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Return)?;
        let expr = if self.is(&TokenKind::Newline) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.eat(TokenKind::Newline)?;
        Ok(Stmt::Return { expr, span })
    }

    /// A leading expression: an assignment if `=` follows a valid target,
    /// otherwise a bare expression statement.
    fn parse_expr_or_assign(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        let expr = self.parse_expr()?;
        if self.is(&TokenKind::Eq) {
            self.advance();
            let value = self.parse_expr()?;
            self.eat(TokenKind::Newline)?;
            self.require_target(&expr)?;
            Ok(Stmt::Assign {
                target: expr,
                expr: value,
                flavored: false,
                span,
            })
        } else {
            self.eat(TokenKind::Newline)?;
            Ok(Stmt::ExprStmt { expr })
        }
    }

    /// A block: `NEWLINE INDENT { statement } DEDENT` (DESIGN §5).
    fn parse_block(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
        self.eat(TokenKind::Newline)?;
        self.eat(TokenKind::Indent).map_err(|_| {
            let span = self.current_span();
            self.diag(span, "doge expected an indented block here")
                .with_headline("very flat. much empty.")
                .with_hint("indent the body with spaces")
        })?;
        let mut stmts = Vec::new();
        while !self.is(&TokenKind::Dedent) {
            if self.is(&TokenKind::Eof) {
                // The lexer balances every Indent with a Dedent, so this only
                // fires on a genuinely truncated stream — treat it as end early.
                return Err(self.missing_wow(self.current_span(), "block"));
            }
            stmts.push(self.parse_statement()?);
        }
        self.eat(TokenKind::Dedent)?;
        Ok(stmts)
    }

    fn parse_params(&mut self) -> Result<Vec<String>, Diagnostic> {
        let mut params = Vec::new();
        loop {
            let (name, _) = self.eat_ident("a parameter name")?;
            params.push(name);
            if self.is(&TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(params)
    }

    fn require_target(&self, expr: &Expr) -> Result<(), Diagnostic> {
        match expr {
            Expr::Ident { .. } | Expr::Index { .. } | Expr::Attr { .. } => Ok(()),
            other => Err(self
                .diag(other.span(), "doge cannot assign to this")
                .with_hint("assign to a name, an item like xs[0], or a field like x.name")),
        }
    }

    // ----- expressions (lowest to highest precedence) -----

    fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.parse_and()?;
        while self.is(&TokenKind::Or) {
            let span = self.current_span();
            self.advance();
            let rhs = self.parse_and()?;
            lhs = Expr::Binary {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.parse_not()?;
        while self.is(&TokenKind::And) {
            let span = self.current_span();
            self.advance();
            let rhs = self.parse_not()?;
            lhs = Expr::Binary {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_not(&mut self) -> Result<Expr, Diagnostic> {
        if self.is(&TokenKind::Not) {
            let span = self.current_span();
            self.advance();
            let operand = self.parse_not()?;
            Ok(Expr::Unary {
                op: UnOp::Not,
                operand: Box::new(operand),
                span,
            })
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> Result<Expr, Diagnostic> {
        let lhs = self.parse_add()?;
        if let Some(op) = comparison_op(self.peek()) {
            let span = self.current_span();
            self.advance();
            let rhs = self.parse_add()?;
            // Non-chaining: `1 < x < 10` is a friendly error (M2 decision 2).
            if comparison_op(self.peek()).is_some() {
                let bad = self.current_span();
                return Err(self
                    .diag(bad, "doge does not chain comparisons like this")
                    .with_hint("use and — 1 < x and x < 10"));
            }
            Ok(Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            })
        } else {
            Ok(lhs)
        }
    }

    fn parse_add(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            let span = self.current_span();
            self.advance();
            let rhs = self.parse_mul()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::SlashSlash => BinOp::FloorDiv,
                TokenKind::Percent => BinOp::Rem,
                _ => break,
            };
            let span = self.current_span();
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, Diagnostic> {
        if self.is(&TokenKind::Minus) {
            let span = self.current_span();
            self.advance();
            let operand = self.parse_unary()?;
            Ok(Expr::Unary {
                op: UnOp::Neg,
                operand: Box::new(operand),
                span,
            })
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                TokenKind::LParen => {
                    let span = self.current_span();
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.eat(TokenKind::RParen)?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                        span,
                    };
                }
                TokenKind::LBracket => {
                    let span = self.current_span();
                    self.advance();
                    let index = self.parse_expr()?;
                    self.eat(TokenKind::RBracket)?;
                    expr = Expr::Index {
                        obj: Box::new(expr),
                        index: Box::new(index),
                        span,
                    };
                }
                TokenKind::Dot => {
                    let span = self.current_span();
                    self.advance();
                    let (name, _) = self.eat_ident("a field or method name after .")?;
                    expr = Expr::Attr {
                        obj: Box::new(expr),
                        name,
                        span,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>, Diagnostic> {
        let mut args = Vec::new();
        loop {
            if self.is(&TokenKind::RParen) {
                break;
            }
            args.push(self.parse_expr()?);
            if self.is(&TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr, Diagnostic> {
        let span = self.current_span();
        match self.peek().clone() {
            TokenKind::Int(value) => {
                self.advance();
                Ok(Expr::Int { value, span })
            }
            TokenKind::Float(value) => {
                self.advance();
                Ok(Expr::Float { value, span })
            }
            TokenKind::Str(value) => {
                self.advance();
                Ok(Expr::Str { value, span })
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool { value: true, span })
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool { value: false, span })
            }
            TokenKind::None => {
                self.advance();
                Ok(Expr::None { span })
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(Expr::Ident { name, span })
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.eat(TokenKind::RParen)?;
                Ok(inner)
            }
            TokenKind::LBracket => self.parse_list(span),
            TokenKind::LBrace => self.parse_dict(span),
            _ => Err(self
                .diag(
                    span,
                    format!(
                        "doge expected a value here, but found {}",
                        self.peek().describe()
                    ),
                )
                .with_hint("a value is a number, string, name, list, or dict")),
        }
    }

    fn parse_list(&mut self, span: Span) -> Result<Expr, Diagnostic> {
        self.eat(TokenKind::LBracket)?;
        let mut items = Vec::new();
        loop {
            if self.is(&TokenKind::RBracket) {
                break;
            }
            items.push(self.parse_expr()?);
            if self.is(&TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.eat(TokenKind::RBracket)?;
        Ok(Expr::List { items, span })
    }

    fn parse_dict(&mut self, span: Span) -> Result<Expr, Diagnostic> {
        self.eat(TokenKind::LBrace)?;
        let mut entries = Vec::new();
        loop {
            if self.is(&TokenKind::RBrace) {
                break;
            }
            let key = self.parse_expr()?;
            self.eat(TokenKind::Colon)?;
            let value = self.parse_expr()?;
            entries.push((key, value));
            if self.is(&TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.eat(TokenKind::RBrace)?;
        Ok(Expr::Dict { entries, span })
    }
}

/// Map a token to its comparison [`BinOp`], if it is one.
fn comparison_op(kind: &TokenKind) -> Option<BinOp> {
    match kind {
        TokenKind::EqEq => Some(BinOp::Eq),
        TokenKind::NotEq => Some(BinOp::NotEq),
        TokenKind::Lt => Some(BinOp::Lt),
        TokenKind::LtEq => Some(BinOp::LtEq),
        TokenKind::Gt => Some(BinOp::Gt),
        TokenKind::GtEq => Some(BinOp::GtEq),
        _ => None,
    }
}

/// The span a statement begins at — used to point diagnostics at a statement
/// discovered during a post-parse walk (e.g. a non-method inside `many`).
fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
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
        | Stmt::Bork { span }
        | Stmt::Continue { span } => *span,
        Stmt::ExprStmt { expr } => expr.span(),
    }
}

#[cfg(test)]
mod tests {
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
}
