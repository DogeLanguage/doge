use super::*;

/// A call's parsed arguments: positional expressions, then `(name, value)`
/// keyword arguments in source order.
type CallArgs = (Vec<Expr>, Vec<(String, Expr)>);

impl Parser {
    // ----- expressions (lowest to highest precedence) -----

    pub(super) fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.parse_ternary()
    }

    /// `then if cond else otherwise` (Python's conditional expression). The
    /// condition and the `then` value are `or`-level; the `else` branch recurses
    /// so `a if p else b if q else c` nests to the right.
    pub(super) fn parse_ternary(&mut self) -> Result<Expr, Diagnostic> {
        let then = self.parse_or()?;
        if self.is(&TokenKind::If) {
            let span = self.current_span();
            self.advance();
            let cond = self.parse_or()?;
            self.eat(TokenKind::Else).map_err(|_| {
                self.diag(self.current_span(), "this a if b needs an else branch")
                    .with_headline("very half. much ternary.")
                    .with_hint("a if cond else b — the else is required")
            })?;
            let otherwise = self.parse_ternary()?;
            Ok(Expr::Ternary {
                cond: Box::new(cond),
                then: Box::new(then),
                otherwise: Box::new(otherwise),
                span,
            })
        } else {
            Ok(then)
        }
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
        let lhs = self.parse_bitor()?;
        if let Some((op, tokens)) = self.peek_comparison() {
            let span = self.current_span();
            for _ in 0..tokens {
                self.advance();
            }
            let rhs = self.parse_bitor()?;
            // Non-chaining: `1 < x < 10` is a friendly error.
            if self.peek_comparison().is_some() {
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

    /// The comparison operator at the cursor and how many tokens it spans, or
    /// `None`. All single-token operators span one; the membership `not in`
    /// spans two (`not` immediately followed by `in`).
    fn peek_comparison(&self) -> Option<(BinOp, usize)> {
        if let Some(op) = comparison_op(self.peek()) {
            return Some((op, 1));
        }
        if self.is(&TokenKind::Not) && self.peek_next() == &TokenKind::In {
            return Some((BinOp::NotIn, 2));
        }
        None
    }

    // Bitwise precedence, loosest to tightest (Python order): `|` then `^` then
    // `&` then the shifts, all between comparison and `+`/`-`.
    pub(super) fn parse_bitor(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.parse_bitxor()?;
        while self.is(&TokenKind::Pipe) {
            let span = self.current_span();
            self.advance();
            let rhs = self.parse_bitxor()?;
            lhs = Expr::Binary {
                op: BinOp::BitOr,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    pub(super) fn parse_bitxor(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.parse_bitand()?;
        while self.is(&TokenKind::Caret) {
            let span = self.current_span();
            self.advance();
            let rhs = self.parse_bitand()?;
            lhs = Expr::Binary {
                op: BinOp::BitXor,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    pub(super) fn parse_bitand(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.parse_shift()?;
        while self.is(&TokenKind::Amp) {
            let span = self.current_span();
            self.advance();
            let rhs = self.parse_shift()?;
            lhs = Expr::Binary {
                op: BinOp::BitAnd,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    pub(super) fn parse_shift(&mut self) -> Result<Expr, Diagnostic> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.peek() {
                TokenKind::Shl => BinOp::Shl,
                TokenKind::Shr => BinOp::Shr,
                _ => break,
            };
            let span = self.current_span();
            self.advance();
            let rhs = self.parse_add()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
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
        let op = match self.peek() {
            TokenKind::Minus => UnOp::Neg,
            TokenKind::Tilde => UnOp::BitNot,
            _ => return self.parse_power(),
        };
        let span = self.current_span();
        self.advance();
        let operand = self.parse_unary()?;
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand),
            span,
        })
    }

    /// `base ** exponent` — right-associative, and binding tighter than unary on
    /// its left (`-2 ** 2` is `-(2 ** 2)`) but looser on its right, since the
    /// exponent is a full unary expression (`2 ** -1`).
    pub(super) fn parse_power(&mut self) -> Result<Expr, Diagnostic> {
        let base = self.parse_postfix()?;
        if self.is(&TokenKind::StarStar) {
            let span = self.current_span();
            self.advance();
            let exponent = self.parse_unary()?;
            Ok(Expr::Binary {
                op: BinOp::Pow,
                lhs: Box::new(base),
                rhs: Box::new(exponent),
                span,
            })
        } else {
            Ok(base)
        }
    }

    pub(super) fn parse_postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                TokenKind::LParen => {
                    let span = self.current_span();
                    self.advance();
                    let (args, kwargs) = self.parse_call_args()?;
                    self.eat(TokenKind::RParen)?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                        kwargs,
                        span,
                    };
                }
                TokenKind::LBracket => {
                    let span = self.current_span();
                    self.advance();
                    expr = self.parse_subscript(expr, span)?;
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

    /// The subscript after a consumed `[`: a plain index `obj[e]`, or a slice
    /// `obj[start:end:step]` where every part is optional. The opening `[` is
    /// already eaten; this consumes through the closing `]`.
    pub(super) fn parse_subscript(&mut self, obj: Expr, span: Span) -> Result<Expr, Diagnostic> {
        // No leading `:` means a start expression is present. `obj[]` reaches
        // parse_expr on `]` and yields the usual "expected a value" error.
        let start = if self.is(&TokenKind::Colon) {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };

        if !self.is(&TokenKind::Colon) {
            self.eat(TokenKind::RBracket)?;
            let index = start.expect("compiler bug: a non-slice subscript has a start expr");
            return Ok(Expr::Index {
                obj: Box::new(obj),
                index,
                span,
            });
        }

        self.advance(); // first ':'
        let end = if self.is(&TokenKind::Colon) || self.is(&TokenKind::RBracket) {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };
        let step = if self.is(&TokenKind::Colon) {
            self.advance(); // second ':'
            if self.is(&TokenKind::RBracket) {
                None
            } else {
                Some(Box::new(self.parse_expr()?))
            }
        } else {
            None
        };
        self.eat(TokenKind::RBracket)?;
        Ok(Expr::Slice {
            obj: Box::new(obj),
            start,
            end,
            step,
            span,
        })
    }

    /// Call arguments after the consumed `(`: positional arguments, then any
    /// keyword arguments `name = value`. A positional argument may not follow a
    /// keyword one, and a keyword name may not repeat (docs/GRAMMAR.md).
    pub(super) fn parse_call_args(&mut self) -> Result<CallArgs, Diagnostic> {
        let mut args = Vec::new();
        let mut kwargs: Vec<(String, Expr)> = Vec::new();
        loop {
            if self.is(&TokenKind::RParen) {
                break;
            }
            // `name = value` — a keyword argument. Any other leading form is a
            // positional expression (which never begins with `IDENT =`).
            if matches!(self.peek(), TokenKind::Ident(_)) && self.peek_next() == &TokenKind::Eq {
                let (name, span) = self.eat_ident("a keyword argument name")?;
                self.eat(TokenKind::Eq)?;
                let value = self.parse_expr()?;
                if kwargs.iter().any(|(n, _)| n == &name) {
                    return Err(self
                        .diag(span, format!("keyword argument {name} is given twice"))
                        .with_headline("very keyword. much repeat.")
                        .with_hint("pass each keyword argument once"));
                }
                kwargs.push((name, value));
            } else {
                if !kwargs.is_empty() {
                    let span = self.current_span();
                    return Err(self
                        .diag(
                            span,
                            "a positional argument cannot follow a keyword argument",
                        )
                        .with_headline("very order. much muddle.")
                        .with_hint("put positional arguments before keyword ones"));
                }
                args.push(self.parse_expr()?);
            }
            if self.is(&TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok((args, kwargs))
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

    /// `super.method(args)` — a call to a parent method, resolved statically. The
    /// `super` token is already at the cursor. `super` on its own, or a bare
    /// `super.field` with no call, is a friendly error: it exists only to call up.
    fn parse_super(&mut self, span: Span) -> Result<Expr, Diagnostic> {
        self.advance(); // super
        self.eat(TokenKind::Dot).map_err(|_| {
            self.diag(span, "super only calls a parent method")
                .with_headline("very super. much confuse.")
                .with_hint("call a parent method — super.init(…)")
        })?;
        let (method, _) = self.eat_ident("a parent method name after super.")?;
        if !self.is(&TokenKind::LParen) {
            return Err(self
                .diag(self.current_span(), "super only calls a parent method")
                .with_headline("very super. much confuse.")
                .with_hint(format!("call it — super.{method}(…)")));
        }
        self.eat(TokenKind::LParen)?;
        let (args, kwargs) = self.parse_call_args()?;
        self.eat(TokenKind::RParen)?;
        if !kwargs.is_empty() {
            return Err(self
                .diag(span, "super passes its arguments positionally")
                .with_headline("very keyword. much dynamic.")
                .with_hint(format!("drop the names — super.{method}(…)")));
        }
        Ok(Expr::SuperCall { method, args, span })
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
            TokenKind::Super => self.parse_super(span),
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
        TokenKind::In => Some(BinOp::In),
        _ => None,
    }
}
