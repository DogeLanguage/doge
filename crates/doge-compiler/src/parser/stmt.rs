use super::*;

impl Parser {
    // ----- statements -----

    pub(super) fn parse_statement(&mut self) -> Result<Stmt, Diagnostic> {
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
            TokenKind::Bonk => self.parse_bonk(),
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
            TokenKind::Amaze => self.parse_amaze(),
            _ => self.parse_expr_or_assign(),
        }
    }

    /// `such` is contextual: `such NAME =` is a variable, `such NAME :` or
    /// `such NAME much …:` is a function definition (docs/GRAMMAR.md).
    pub(super) fn parse_such(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Such)?;
        let (name, _) = self.eat_ident("a name after such")?;

        // `such a, b = …` — a destructuring declaration. A name list is always a
        // declaration (a function has a single name), so `=` and values follow.
        if self.is(&TokenKind::Comma) {
            let (names, rest) = self.parse_target_names(name)?;
            self.expect_destructure_eq()?;
            let expr = self.parse_assign_rhs(true)?;
            self.eat(TokenKind::Newline)?;
            return Ok(Stmt::Decl {
                names,
                rest,
                expr,
                span,
            });
        }

        match self.peek() {
            TokenKind::Eq => {
                self.advance();
                let expr = self.parse_assign_rhs(false)?;
                self.eat(TokenKind::Newline)?;
                Ok(Stmt::Decl {
                    names: vec![name],
                    rest: None,
                    expr,
                    span,
                })
            }
            TokenKind::Much | TokenKind::Colon => {
                let mut params = Params::default();
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
    /// import (docs/GRAMMAR.md).
    pub(super) fn parse_so(&mut self) -> Result<Stmt, Diagnostic> {
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

    /// `many NAME [much PARENT]: … wow` — an object definition whose body is only
    /// functions. The optional `much PARENT` names the class it inherits from.
    pub(super) fn parse_many(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Many)?;
        let (name, _) = self.eat_ident("an object name after many")?;
        let parent = if self.is(&TokenKind::Much) {
            self.advance();
            let (parent, _) = self.eat_ident("a parent object name after much")?;
            Some(parent)
        } else {
            None
        };
        self.eat(TokenKind::Colon)?;
        let body = self.parse_block()?;
        let mut methods = Vec::new();
        for stmt in body {
            match stmt {
                Stmt::FuncDef { .. } => methods.push(stmt),
                other => {
                    let bad = other.span();
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
            parent,
            methods,
            span,
        })
    }

    pub(super) fn parse_very(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Very)?;
        let first = self.parse_expr()?;
        if self.is(&TokenKind::Comma) {
            return self.finish_multi_assign(first, true, span);
        }
        let op = match self.peek() {
            TokenKind::Eq => None,
            TokenKind::AugAssign(op) => Some(*op),
            _ => {
                return Err(self
                    .diag(
                        first.span(),
                        "very always reassigns — doge expects = after the target",
                    )
                    .with_hint("very age = 9"))
            }
        };
        self.advance();
        let expr = self.parse_assign_rhs(false)?;
        self.eat(TokenKind::Newline)?;
        self.require_target(&first)?;
        Ok(Stmt::Assign {
            targets: vec![first],
            rest: None,
            expr,
            op,
            flavored: true,
            span,
        })
    }

    pub(super) fn parse_pls(&mut self) -> Result<Stmt, Diagnostic> {
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

    pub(super) fn parse_if(&mut self) -> Result<Stmt, Diagnostic> {
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

    pub(super) fn parse_for(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::For)?;
        let (first, _) = self.eat_ident("a loop variable after for")?;
        let (vars, rest) = if self.is(&TokenKind::Comma) {
            self.parse_target_names(first)?
        } else {
            (vec![first], None)
        };
        self.eat(TokenKind::In)?;
        let iter = self.parse_expr()?;
        self.eat(TokenKind::Colon)?;
        let body = self.parse_block()?;
        Ok(Stmt::For {
            vars,
            rest,
            iter,
            body,
            span,
        })
    }

    pub(super) fn parse_while(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::While)?;
        let cond = self.parse_expr()?;
        self.eat(TokenKind::Colon)?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body, span })
    }

    pub(super) fn parse_bark(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Bark)?;
        let expr = self.parse_expr()?;
        self.eat(TokenKind::Newline)?;
        Ok(Stmt::Bark { expr, span })
    }

    pub(super) fn parse_bonk(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Bonk)?;
        let expr = self.parse_expr()?;
        self.eat(TokenKind::Newline)?;
        Ok(Stmt::Bonk { expr, span })
    }

    /// `amaze cond` or `amaze cond, message` — assert. The optional message
    /// follows a comma; `parse_expr` stops at the comma, so the split is clean.
    pub(super) fn parse_amaze(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        self.eat(TokenKind::Amaze)?;
        let cond = self.parse_expr()?;
        let message = if self.is(&TokenKind::Comma) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.eat(TokenKind::Newline)?;
        Ok(Stmt::Amaze {
            cond,
            message,
            span,
        })
    }

    pub(super) fn parse_return(&mut self) -> Result<Stmt, Diagnostic> {
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
    pub(super) fn parse_expr_or_assign(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.current_span();
        let expr = self.parse_expr()?;
        if self.is(&TokenKind::Comma) {
            return self.finish_multi_assign(expr, false, span);
        }
        // `Some(None)` is a plain `=`; `Some(Some(op))` is an augmented `op=`.
        let assign = match self.peek() {
            TokenKind::Eq => Some(None),
            TokenKind::AugAssign(op) => Some(Some(*op)),
            _ => None,
        };
        if let Some(op) = assign {
            self.advance();
            let value = self.parse_assign_rhs(false)?;
            self.eat(TokenKind::Newline)?;
            self.require_target(&expr)?;
            Ok(Stmt::Assign {
                targets: vec![expr],
                rest: None,
                expr: value,
                op,
                flavored: false,
                span,
            })
        } else {
            self.eat(TokenKind::Newline)?;
            Ok(Stmt::ExprStmt { expr })
        }
    }

    /// Finish a destructuring assignment once a `,` after the first target has
    /// revealed it: collect the remaining targets (with an optional trailing
    /// `many` collector), reject an augmented operator (destructuring is `=`
    /// only), then bind the comma-list right-hand side. `flavored` records a
    /// leading `very`.
    fn finish_multi_assign(
        &mut self,
        first: Expr,
        flavored: bool,
        span: Span,
    ) -> Result<Stmt, Diagnostic> {
        let (targets, rest) = self.parse_target_exprs(first)?;
        if let TokenKind::AugAssign(_) = self.peek() {
            return Err(self
                .diag(
                    self.current_span(),
                    "augmented assignment takes a single target",
                )
                .with_headline("very many. much augment.")
                .with_hint("split it up, e.g. a = a + 1 on its own line"));
        }
        self.expect_destructure_eq()?;
        let value = self.parse_assign_rhs(true)?;
        self.eat(TokenKind::Newline)?;
        for target in &targets {
            self.require_target(target)?;
        }
        if let Some(rest) = &rest {
            self.require_target(rest)?;
        }
        Ok(Stmt::Assign {
            targets,
            rest,
            expr: value,
            op: None,
            flavored,
            span,
        })
    }

    /// A block: `NEWLINE INDENT { statement } DEDENT` (docs/GRAMMAR.md).
    pub(super) fn parse_block(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
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

    /// A function header's parameter list (after the `much`): comma-separated
    /// parameters, each optionally `name = literal`, with an optional trailing
    /// `many rest` variadic. Required parameters must precede defaulted ones, and
    /// the variadic — if present — must come last (docs/GRAMMAR.md).
    pub(super) fn parse_params(&mut self) -> Result<Params, Diagnostic> {
        let mut params: Vec<Param> = Vec::new();
        let mut vararg: Option<String> = None;
        loop {
            if self.is(&TokenKind::Many) {
                self.advance();
                let (name, _) = self.eat_ident("a name after many")?;
                vararg = Some(name);
                if self.is(&TokenKind::Comma) {
                    let comma = self.current_span();
                    return Err(self
                        .diag(comma, "the many parameter must be the last one")
                        .with_headline("very rest. much greedy.")
                        .with_hint("put many rest at the end of the parameter list"));
                }
                break;
            }
            let (name, span) = self.eat_ident("a parameter name")?;
            let default = if self.is(&TokenKind::Eq) {
                self.advance();
                let expr = self.parse_expr()?;
                self.require_literal(&expr)?;
                Some(expr)
            } else {
                if params.last().is_some_and(|p| p.default.is_some()) {
                    return Err(self
                        .diag(
                            span,
                            "a parameter with no default cannot follow one with a default",
                        )
                        .with_headline("very order. much default.")
                        .with_hint("move defaulted parameters to the end of the list"));
                }
                None
            };
            params.push(Param {
                name,
                default,
                span,
            });
            if self.is(&TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(Params { params, vararg })
    }

    /// A default parameter value must be a literal: a number, string, bool,
    /// `none`, a unary minus on a number, or a list/dict of literals. This keeps a
    /// default free of names and side effects, so it is evaluated fresh at every
    /// call (docs/SYNTAX.md §6).
    pub(super) fn require_literal(&self, expr: &Expr) -> Result<(), Diagnostic> {
        let ok = match expr {
            Expr::Int { .. }
            | Expr::Float { .. }
            | Expr::Str { .. }
            | Expr::Bool { .. }
            | Expr::None { .. } => true,
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => matches!(operand.as_ref(), Expr::Int { .. } | Expr::Float { .. }),
            Expr::List { items, .. } => {
                for item in items {
                    self.require_literal(item)?;
                }
                true
            }
            Expr::Dict { entries, .. } => {
                for (key, value) in entries {
                    self.require_literal(key)?;
                    self.require_literal(value)?;
                }
                true
            }
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(self
                .diag(expr.span(), "a default parameter value must be a literal")
                .with_headline("very default. much dynamic.")
                .with_hint("use a fixed value like 0, \"hi\", true, none, or [ ]"))
        }
    }

    pub(super) fn require_target(&self, expr: &Expr) -> Result<(), Diagnostic> {
        match expr {
            Expr::Ident { .. } | Expr::Index { .. } | Expr::Attr { .. } => Ok(()),
            other => Err(self
                .diag(other.span(), "doge cannot assign to this")
                .with_hint("assign to a name, an item like xs[0], or a field like x.name")),
        }
    }

    /// The remaining names of a destructuring `such`/`for` header, given the
    /// already-consumed `first` name and the `,` that follows it: each further
    /// comma-separated name, then an optional trailing `many rest` collector that
    /// must be the last target. Used where every target is a plain binding name.
    fn parse_target_names(
        &mut self,
        first: String,
    ) -> Result<(Vec<String>, Option<String>), Diagnostic> {
        let mut names = vec![first];
        let mut rest = None;
        while self.is(&TokenKind::Comma) {
            self.advance();
            if self.is(&TokenKind::Many) {
                self.advance();
                let (name, _) = self.eat_ident("a name after many")?;
                rest = Some(name);
                self.reject_target_after_collector()?;
                break;
            }
            let (name, _) = self.eat_ident("a name after the comma")?;
            names.push(name);
        }
        Ok((names, rest))
    }

    /// Like [`parse_target_names`] but for a reassignment, where each target is a
    /// full assignable expression (`a`, `xs[0]`, `dog.name`) rather than a plain
    /// name. `require_target` validates each one at the call site.
    fn parse_target_exprs(&mut self, first: Expr) -> Result<(Vec<Expr>, Option<Expr>), Diagnostic> {
        let mut targets = vec![first];
        let mut rest = None;
        while self.is(&TokenKind::Comma) {
            self.advance();
            if self.is(&TokenKind::Many) {
                self.advance();
                rest = Some(self.parse_expr()?);
                self.reject_target_after_collector()?;
                break;
            }
            targets.push(self.parse_expr()?);
        }
        Ok((targets, rest))
    }

    /// A `many rest` collector must be the final target: a `,` after it is an
    /// error, mirroring the same rule on a `much rest` function parameter.
    fn reject_target_after_collector(&self) -> Result<(), Diagnostic> {
        if self.is(&TokenKind::Comma) {
            return Err(self
                .diag(
                    self.current_span(),
                    "the many collector must be the last target",
                )
                .with_headline("very rest. much greedy.")
                .with_hint("put many rest at the end of the target list"));
        }
        Ok(())
    }

    /// The right-hand side of an assignment. In a multiple-assignment position
    /// (`multi`), a comma-separated list of values builds an implicit List, so
    /// `a, b = b, a` swaps; a single value is returned as-is (unpacked at
    /// runtime). Outside multiple assignment a trailing comma is an error, so
    /// `such x = 1, 2` stays rejected rather than silently building a List.
    fn parse_assign_rhs(&mut self, multi: bool) -> Result<Expr, Diagnostic> {
        let first = self.parse_expr()?;
        if !self.is(&TokenKind::Comma) {
            return Ok(first);
        }
        if !multi {
            return Err(self
                .diag(
                    self.current_span(),
                    "doge sees extra values but only one name to hold them",
                )
                .with_headline("very many. much value.")
                .with_hint("give the left side matching names, or wrap the values in [ ]"));
        }
        let span = first.span();
        let mut items = vec![first];
        while self.is(&TokenKind::Comma) {
            self.advance();
            items.push(self.parse_expr()?);
        }
        Ok(Expr::List { items, span })
    }

    /// A destructuring header's target list must be followed by `=` and values.
    fn expect_destructure_eq(&mut self) -> Result<(), Diagnostic> {
        if !self.is(&TokenKind::Eq) {
            return Err(self
                .diag(
                    self.current_span(),
                    "a destructuring assignment needs = and values",
                )
                .with_headline("very unpack. much incomplete.")
                .with_hint("give matching values, e.g. a, b = [1, 2]"));
        }
        self.advance();
        Ok(())
    }
}
