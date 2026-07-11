use super::*;

impl Parser {
    // ----- expressions (lowest to highest precedence) -----

    pub(super) fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.parse_or()
    }

    pub(super) fn parse_or(&mut self) -> Result<Expr, Diagnostic> {
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

    pub(super) fn parse_and(&mut self) -> Result<Expr, Diagnostic> {
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

    pub(super) fn parse_not(&mut self) -> Result<Expr, Diagnostic> {
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

    pub(super) fn parse_comparison(&mut self) -> Result<Expr, Diagnostic> {
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

    pub(super) fn parse_add(&mut self) -> Result<Expr, Diagnostic> {
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

    pub(super) fn parse_mul(&mut self) -> Result<Expr, Diagnostic> {
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

    pub(super) fn parse_unary(&mut self) -> Result<Expr, Diagnostic> {
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

    pub(super) fn parse_postfix(&mut self) -> Result<Expr, Diagnostic> {
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

    pub(super) fn parse_call_args(&mut self) -> Result<Vec<Expr>, Diagnostic> {
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

    /// Turn a lexed interpolated string into an [`Expr::StrInterp`]: literal
    /// segments pass through, and each `{…}` hole's tokens are parsed as a full
    /// expression by a sub-parser. A hole that holds more than one expression is
    /// a diagnostic anchored at the first leftover token.
    pub(super) fn parse_str_interp(
        &mut self,
        segments: Vec<StrSegment>,
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let mut parts = Vec::with_capacity(segments.len());
        for segment in segments {
            match segment {
                StrSegment::Lit(text) => parts.push(InterpPart::Lit(text)),
                StrSegment::Hole(mut tokens) => {
                    let end_span = tokens.last().map(|t| t.span).unwrap_or(span);
                    tokens.push(Token {
                        kind: TokenKind::Eof,
                        span: end_span,
                    });
                    let mut sub = self.sub(tokens);
                    let expr = sub.parse_expr()?;
                    if !sub.is(&TokenKind::Eof) {
                        let extra = sub.current_span();
                        return Err(sub.diag(
                            extra,
                            format!(
                                "doge expected one expression in this {{…}} hole, but found {}",
                                sub.peek().describe()
                            ),
                        ));
                    }
                    parts.push(InterpPart::Expr(expr));
                }
            }
        }
        Ok(Expr::StrInterp { parts, span })
    }

    pub(super) fn parse_primary(&mut self) -> Result<Expr, Diagnostic> {
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
            TokenKind::StrInterp(segments) => {
                self.advance();
                self.parse_str_interp(segments, span)
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

    pub(super) fn parse_list(&mut self, span: Span) -> Result<Expr, Diagnostic> {
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

    pub(super) fn parse_dict(&mut self, span: Span) -> Result<Expr, Diagnostic> {
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
