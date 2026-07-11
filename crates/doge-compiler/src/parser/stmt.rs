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

    /// `such` is contextual: `such NAME =` is a variable, `such NAME :` or
    /// `such NAME much …:` is a function definition (docs/GRAMMAR.md).
    pub(super) fn parse_such(&mut self) -> Result<Stmt, Diagnostic> {
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

    /// `many NAME: … wow` — an object definition whose body is only functions.
    pub(super) fn parse_many(&mut self) -> Result<Stmt, Diagnostic> {
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
            methods,
            span,
        })
    }

    pub(super) fn parse_very(&mut self) -> Result<Stmt, Diagnostic> {
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

    pub(super) fn parse_params(&mut self) -> Result<Vec<String>, Diagnostic> {
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

    pub(super) fn require_target(&self, expr: &Expr) -> Result<(), Diagnostic> {
        match expr {
            Expr::Ident { .. } | Expr::Index { .. } | Expr::Attr { .. } => Ok(()),
            other => Err(self
                .diag(other.span(), "doge cannot assign to this")
                .with_hint("assign to a name, an item like xs[0], or a field like x.name")),
        }
    }
}
