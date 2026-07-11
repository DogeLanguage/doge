pub(super) use crate::ast::{BinOp, Expr, InterpPart, Script, Stmt, UnOp};
pub(super) use crate::diagnostics::Diagnostic;
use crate::lexer;
pub(super) use crate::token::{Span, StrSegment, Token, TokenKind};

mod expr;
mod stmt;
#[cfg(test)]
mod tests;

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

    /// A fresh parser over `tokens`, sharing this parser's path and source lines
    /// so diagnostics still render the right file and line. Used to parse the
    /// expression inside a `{…}` interpolation hole.
    fn sub(&self, tokens: Vec<Token>) -> Parser {
        Parser {
            path: self.path.clone(),
            lines: self.lines.clone(),
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
        let source_line = crate::diagnostics::source_line(&self.lines, span.line);
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

    fn python_habit(&self, message: &str, hint: &str) -> Diagnostic {
        let span = self.current_span();
        self.diag(span, message)
            .with_headline("very python. much habit.")
            .with_hint(hint)
    }
}
