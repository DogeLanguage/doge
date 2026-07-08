use crate::diagnostics::Diagnostic;
use crate::keywords;
use crate::token::{Span, Token, TokenKind};

/// Lex `source` (from `path`) into a token stream, or return the first
/// [`Diagnostic`] encountered.
pub fn lex(path: &str, source: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(path, source).run()
}

struct Lexer {
    path: String,
    /// Physical lines, `\r` stripped, indexed 0-based (line N is `lines[N-1]`).
    lines: Vec<String>,
    /// Indentation widths of currently open blocks; always starts with `0`.
    indent_stack: Vec<usize>,
    /// Spans of the currently open `(` `[` `{`; its length is the bracket depth.
    /// Tracking spans (not just a count) lets EOF point at the unclosed opener.
    bracket_stack: Vec<Span>,
    tokens: Vec<Token>,
}

impl Lexer {
    fn new(path: &str, source: &str) -> Lexer {
        let lines = source
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();
        Lexer {
            path: path.to_string(),
            lines,
            indent_stack: vec![0],
            bracket_stack: Vec::new(),
            tokens: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Vec<Token>, Diagnostic> {
        // Clone the line list up front so the borrow checker lets us read a line
        // while pushing tokens; lines never change during lexing.
        let lines = self.lines.clone();
        for (idx, text) in lines.iter().enumerate() {
            let ln = (idx + 1) as u32;
            let chars: Vec<char> = text.chars().collect();

            let start = if self.bracket_stack.is_empty() {
                match self.begin_logical_line(&chars, ln)? {
                    // A blank or comment line contributes nothing.
                    None => continue,
                    Some(start) => start,
                }
            } else {
                // Inside brackets: skip leading whitespace, no indentation work.
                let mut i = 0;
                while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                    i += 1;
                }
                if i >= chars.len() || chars[i] == '#' {
                    continue;
                }
                i
            };

            self.lex_line(&chars, ln, start)?;

            // A logical line ends only when no bracket is open (implicit joining).
            if self.bracket_stack.is_empty() {
                self.push(TokenKind::Newline, ln, (chars.len() + 1) as u32);
            }
        }

        // Close any blocks still open at end of file, then mark the end.
        let last_line = self.lines.len() as u32;
        if let Some(open) = self.bracket_stack.first() {
            let span = *open;
            return Err(self
                .diag(span.line, span.col, "this bracket was never closed")
                .with_headline("very open. much bracket.")
                .with_hint("add the matching closing bracket"));
        }
        while *self
            .indent_stack
            .last()
            .expect("compiler bug: indent stack keeps its base 0")
            > 0
        {
            self.indent_stack.pop();
            self.push(TokenKind::Dedent, last_line, 1);
        }
        self.push(TokenKind::Eof, last_line, 1);
        Ok(self.tokens)
    }

    /// Handle the indentation of a fresh logical line (bracket depth 0). Returns
    /// `Ok(None)` for blank/comment lines (which emit nothing) or `Ok(Some(i))`
    /// with the index of the first content character.
    fn begin_logical_line(&mut self, chars: &[char], ln: u32) -> Result<Option<usize>, Diagnostic> {
        let mut i = 0;
        let mut tab_col: Option<u32> = None;
        while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
            if chars[i] == '\t' && tab_col.is_none() {
                tab_col = Some((i + 1) as u32);
            }
            i += 1;
        }

        // Blank or comment-only lines never affect indentation (and a tab on
        // such a line is harmless — it indents nothing).
        if i >= chars.len() || chars[i] == '#' {
            return Ok(None);
        }

        if let Some(col) = tab_col {
            return Err(self
                .diag(ln, col, "tabs cannot be used to indent")
                .with_headline("very tab. much confuse.")
                .with_hint("indent with spaces"));
        }

        let count = i; // all leading whitespace is spaces here
        let top = *self
            .indent_stack
            .last()
            .expect("compiler bug: indent stack keeps its base 0");
        if count > top {
            self.indent_stack.push(count);
            self.push(TokenKind::Indent, ln, (count + 1) as u32);
        } else if count < top {
            while *self
                .indent_stack
                .last()
                .expect("compiler bug: indent stack keeps its base 0")
                > count
            {
                self.indent_stack.pop();
                self.push(TokenKind::Dedent, ln, (count + 1) as u32);
            }
            if *self
                .indent_stack
                .last()
                .expect("compiler bug: indent stack keeps its base 0")
                != count
            {
                return Err(self
                    .diag(
                        ln,
                        (count + 1) as u32,
                        "this line does not line up with any block",
                    )
                    .with_headline("very indent. much confuse.")
                    .with_hint("match the indentation of an outer block"));
            }
        }
        Ok(Some(i))
    }

    /// Lex the content of one physical line, from `start` to its end, appending
    /// tokens. Stops at a `#` comment. Bracket depth may change as a side effect.
    fn lex_line(&mut self, chars: &[char], ln: u32, start: usize) -> Result<(), Diagnostic> {
        let mut i = start;
        while i < chars.len() {
            let c = chars[i];
            let col = (i + 1) as u32;

            if c == ' ' || c == '\t' {
                i += 1;
                continue;
            }
            if c == '#' {
                break; // comment runs to end of line
            }

            if c.is_ascii_alphabetic() || c == '_' {
                i = self.lex_word(chars, ln, i);
                continue;
            }
            if c.is_ascii_digit() {
                i = self.lex_number(chars, ln, i)?;
                continue;
            }
            if c == '"' {
                i = self.lex_string(chars, ln, i)?;
                continue;
            }

            i = self.lex_operator(chars, ln, i, col)?;
        }
        Ok(())
    }

    /// Lex an identifier or keyword starting at `start`; also fuses `oh no`.
    /// Returns the index just past the consumed token.
    fn lex_word(&mut self, chars: &[char], ln: u32, start: usize) -> usize {
        let col = (start + 1) as u32;
        let mut end = start;
        while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
            end += 1;
        }
        let word: String = chars[start..end].iter().collect();

        // `oh no` fusion: after reading `oh`, peek the next word on this line.
        if word == "oh" {
            let mut j = end;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            let mut k = j;
            while k < chars.len() && (chars[k].is_ascii_alphanumeric() || chars[k] == '_') {
                k += 1;
            }
            let next: String = chars[j..k].iter().collect();
            if next == "no" {
                self.push(TokenKind::OhNo, ln, col);
                return k;
            }
            // Otherwise `oh` is an ordinary identifier; leave the peeked word.
        }

        match keywords::lookup(&word) {
            Some(kind) => self.push(kind, ln, col),
            None => self.push(TokenKind::Ident(word), ln, col),
        }
        end
    }

    /// Lex an Int or Float literal starting at `start`.
    fn lex_number(&mut self, chars: &[char], ln: u32, start: usize) -> Result<usize, Diagnostic> {
        let col = (start + 1) as u32;
        let mut end = start;
        while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
        }

        // A `.` counts as a decimal point only when a digit follows it, so
        // `1.5` is a Float but `1.foo` is Int `1` then `.` then `foo`.
        let is_float =
            end + 1 < chars.len() && chars[end] == '.' && chars[end + 1].is_ascii_digit();
        if is_float {
            end += 1; // consume '.'
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            let text: String = chars[start..end].iter().collect();
            // A run of ASCII digits with one interior dot always parses as f64.
            let value: f64 = text
                .parse()
                .expect("compiler bug: digit-only float must parse");
            self.push(TokenKind::Float(value), ln, col);
            return Ok(end);
        }

        let text: String = chars[start..end].iter().collect();
        match text.parse::<i64>() {
            Ok(value) => {
                self.push(TokenKind::Int(value), ln, col);
                Ok(end)
            }
            Err(_) => Err(self
                .diag(ln, col, "this whole number is too big to hold")
                .with_headline("very big. much number.")
                .with_hint(format!(
                    "whole numbers must be between {} and {}",
                    i64::MIN,
                    i64::MAX
                ))),
        }
    }

    /// Lex a double-quoted string starting at the opening quote at `start`.
    fn lex_string(&mut self, chars: &[char], ln: u32, start: usize) -> Result<usize, Diagnostic> {
        let col = (start + 1) as u32;
        let mut i = start + 1; // past opening quote
        let mut value = String::new();
        while i < chars.len() {
            let c = chars[i];
            if c == '"' {
                self.push(TokenKind::Str(value), ln, col);
                return Ok(i + 1);
            }
            if c == '\\' {
                let esc_col = (i + 1) as u32;
                i += 1;
                if i >= chars.len() {
                    break; // dangling backslash → unterminated
                }
                match chars[i] {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    other => {
                        return Err(self
                            .diag(
                                ln,
                                esc_col,
                                format!("'\\{other}' is not an escape doge knows"),
                            )
                            .with_hint("known escapes are \\n \\t \\\" and \\\\"));
                    }
                }
                i += 1;
                continue;
            }
            value.push(c);
            i += 1;
        }
        // Reached end of line with no closing quote (a raw newline can't appear
        // inside `chars`, which is a single physical line).
        Err(self
            .diag(ln, col, "this string never closes")
            .with_headline("very string. much unfinished.")
            .with_hint("add a closing \" on this line"))
    }

    /// Lex a single operator or punctuation token. Two-character operators are
    /// checked before their one-character prefixes.
    fn lex_operator(
        &mut self,
        chars: &[char],
        ln: u32,
        i: usize,
        col: u32,
    ) -> Result<usize, Diagnostic> {
        let c = chars[i];
        let next = chars.get(i + 1).copied();

        // Two-character operators first.
        match (c, next) {
            ('/', Some('/')) => {
                self.push(TokenKind::SlashSlash, ln, col);
                return Ok(i + 2);
            }
            ('=', Some('=')) => {
                self.push(TokenKind::EqEq, ln, col);
                return Ok(i + 2);
            }
            ('!', Some('=')) => {
                self.push(TokenKind::NotEq, ln, col);
                return Ok(i + 2);
            }
            ('<', Some('=')) => {
                self.push(TokenKind::LtEq, ln, col);
                return Ok(i + 2);
            }
            ('>', Some('=')) => {
                self.push(TokenKind::GtEq, ln, col);
                return Ok(i + 2);
            }
            _ => {}
        }

        let kind = match c {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '=' => TokenKind::Eq,
            '!' => TokenKind::Bang,
            '<' => TokenKind::Lt,
            '>' => TokenKind::Gt,
            ':' => TokenKind::Colon,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '(' => {
                self.bracket_stack.push(Span { line: ln, col });
                TokenKind::LParen
            }
            '[' => {
                self.bracket_stack.push(Span { line: ln, col });
                TokenKind::LBracket
            }
            '{' => {
                self.bracket_stack.push(Span { line: ln, col });
                TokenKind::LBrace
            }
            ')' => {
                self.bracket_stack.pop();
                TokenKind::RParen
            }
            ']' => {
                self.bracket_stack.pop();
                TokenKind::RBracket
            }
            '}' => {
                self.bracket_stack.pop();
                TokenKind::RBrace
            }
            other => {
                return Err(self
                    .diag(ln, col, format!("'{other}' means nothing to doge here"))
                    .with_hint("remove it, or check for a typo"));
            }
        };
        self.push(kind, ln, col);
        Ok(i + 1)
    }

    fn push(&mut self, kind: TokenKind, line: u32, col: u32) {
        self.tokens.push(Token {
            kind,
            span: Span { line, col },
        });
    }

    /// Build a diagnostic anchored at (line, col), pulling the offending source
    /// line verbatim. A line number past the end falls back to an empty line.
    fn diag(&self, line: u32, col: u32, message: impl Into<String>) -> Diagnostic {
        let source_line = self
            .lines
            .get((line as usize).saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        Diagnostic::new(&self.path, line, col, source_line, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lex and return just the token kinds, panicking on a diagnostic (tests
    /// that expect success).
    fn kinds(source: &str) -> Vec<TokenKind> {
        lex("test.doge", source)
            .expect("expected clean lex")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn indent_dedent_pairs_balance() {
        let toks = kinds("if x:\n    bark y\nwow\n");
        let indents = toks.iter().filter(|k| **k == TokenKind::Indent).count();
        let dedents = toks.iter().filter(|k| **k == TokenKind::Dedent).count();
        assert_eq!(indents, 1);
        assert_eq!(dedents, 1);
        assert_eq!(toks.last(), Some(&TokenKind::Eof));
    }

    #[test]
    fn blank_and_comment_lines_do_not_dedent() {
        // The blank line and comment line inside the block must not emit Dedent.
        let toks = kinds("if x:\n    bark y\n\n    # still inside\n    bark z\nwow\n");
        let dedents = toks.iter().filter(|k| **k == TokenKind::Dedent).count();
        assert_eq!(dedents, 1); // only the real dedent before `wow`
    }

    #[test]
    fn oh_no_fuses_into_one_token() {
        let toks = kinds("oh no err!\n");
        assert_eq!(toks[0], TokenKind::OhNo);
        assert_eq!(toks[1], TokenKind::Ident("err".into()));
        assert_eq!(toks[2], TokenKind::Bang);
    }

    #[test]
    fn oh_alone_is_an_identifier() {
        let toks = kinds("oh = 1\n");
        assert_eq!(toks[0], TokenKind::Ident("oh".into()));
        assert_eq!(toks[1], TokenKind::Eq);
    }

    #[test]
    fn tab_indent_is_an_error() {
        let err = lex("test.doge", "if x:\n\tbark y\n").unwrap_err();
        assert_eq!(err.headline, "very tab. much confuse.");
        assert_eq!(err.line, 2);
        assert_eq!(err.col, 1);
    }

    #[test]
    fn inconsistent_dedent_is_an_error() {
        // Dedent to a column that never opened a block.
        let err = lex("test.doge", "if x:\n        bark y\n    bark z\n").unwrap_err();
        assert_eq!(err.headline, "very indent. much confuse.");
    }

    #[test]
    fn brackets_suppress_newlines() {
        let toks = kinds("such xs = [\n    1,\n    2,\n]\n");
        // Exactly one Newline (after the closing bracket), no Indent/Dedent.
        assert_eq!(toks.iter().filter(|k| **k == TokenKind::Newline).count(), 1);
        assert_eq!(toks.iter().filter(|k| **k == TokenKind::Indent).count(), 0);
        assert_eq!(toks.iter().filter(|k| **k == TokenKind::Dedent).count(), 0);
    }

    #[test]
    fn floordiv_lexes_before_div() {
        let toks = kinds("bark 7 // 2\n");
        assert!(toks.contains(&TokenKind::SlashSlash));
        assert!(!toks.contains(&TokenKind::Slash));
    }

    #[test]
    fn float_needs_a_digit_after_the_dot() {
        let toks = kinds("bark 1.5\n");
        assert_eq!(toks[1], TokenKind::Float(1.5));
        // `1.foo` is Int then Dot then Ident, not a float.
        let toks = kinds("bark 1.foo\n");
        assert_eq!(toks[1], TokenKind::Int(1));
        assert_eq!(toks[2], TokenKind::Dot);
        assert_eq!(toks[3], TokenKind::Ident("foo".into()));
    }

    #[test]
    fn string_escapes_and_unterminated() {
        let toks = kinds("bark \"a\\nb\\t\\\"c\"\n");
        assert_eq!(toks[1], TokenKind::Str("a\nb\t\"c".into()));

        let err = lex("test.doge", "bark \"open\n").unwrap_err();
        assert_eq!(err.headline, "very string. much unfinished.");

        let bad = lex("test.doge", "bark \"a\\qb\"\n").unwrap_err();
        assert!(bad.message.contains("not an escape"));
    }

    #[test]
    fn int_overflow_is_an_error() {
        let err = lex("test.doge", "bark 99999999999999999999999\n").unwrap_err();
        assert_eq!(err.headline, "very big. much number.");
    }

    #[test]
    fn unknown_character_is_an_error() {
        let err = lex("test.doge", "bark ~x\n").unwrap_err();
        assert!(err.message.contains('~'));
    }

    #[test]
    fn unclosed_bracket_is_an_error() {
        let err = lex("test.doge", "such xs = [1, 2\n").unwrap_err();
        assert_eq!(err.headline, "very open. much bracket.");
    }
}
