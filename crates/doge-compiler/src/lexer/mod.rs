pub(super) use crate::diagnostics::Diagnostic;
use crate::keywords;
pub(super) use crate::token::{Span, StrSegment, Token, TokenKind};

mod scan;
mod strings;
#[cfg(test)]
mod tests;

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
        self.lex_range(chars, ln, start, chars.len())
    }

    /// Lex the half-open character range `[start, end)` of one physical line,
    /// appending tokens. Stops at a `#` comment. Used both for a whole line
    /// (`end == chars.len()`) and for the content of a `{…}` interpolation hole.
    fn lex_range(
        &mut self,
        chars: &[char],
        ln: u32,
        start: usize,
        end: usize,
    ) -> Result<(), Diagnostic> {
        let mut i = start;
        while i < end {
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

    fn push(&mut self, kind: TokenKind, line: u32, col: u32) {
        self.tokens.push(Token {
            kind,
            span: Span { line, col },
        });
    }

    /// Build a diagnostic anchored at (line, col), pulling the offending source
    /// line verbatim. A line number past the end falls back to an empty line.
    fn diag(&self, line: u32, col: u32, message: impl Into<String>) -> Diagnostic {
        let source_line = crate::diagnostics::source_line(&self.lines, line);
        Diagnostic::new(&self.path, line, col, source_line, message)
    }
}
