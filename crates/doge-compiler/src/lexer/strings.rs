use super::*;

impl Lexer {
    /// Lex a double-quoted string starting at the opening quote at `start`. A
    /// string with no `{…}` hole becomes a plain [`TokenKind::Str`]; one with at
    /// least one hole becomes a [`TokenKind::StrInterp`] whose holes are lexed
    /// into their own token streams (see [`Self::lex_hole`]).
    pub(super) fn lex_string(
        &mut self,
        chars: &[char],
        ln: u32,
        start: usize,
    ) -> Result<usize, Diagnostic> {
        let col = (start + 1) as u32;
        let mut i = start + 1; // past opening quote
        let mut lit = String::new();
        let mut segments: Vec<StrSegment> = Vec::new();
        while i < chars.len() {
            let c = chars[i];
            if c == '"' {
                if segments.is_empty() {
                    self.push(TokenKind::Str(lit), ln, col);
                } else {
                    if !lit.is_empty() {
                        segments.push(StrSegment::Lit(lit));
                    }
                    self.push(TokenKind::StrInterp(segments), ln, col);
                }
                return Ok(i + 1);
            }
            if c == '\\' {
                let esc_col = (i + 1) as u32;
                i += 1;
                if i >= chars.len() {
                    break; // dangling backslash → unterminated
                }
                match chars[i] {
                    'n' => lit.push('\n'),
                    't' => lit.push('\t'),
                    '"' => lit.push('"'),
                    '\\' => lit.push('\\'),
                    '{' => lit.push('{'),
                    '}' => lit.push('}'),
                    other => {
                        return Err(self
                            .diag(
                                ln,
                                esc_col,
                                format!("'\\{other}' is not an escape doge knows"),
                            )
                            .with_hint("known escapes are \\n \\t \\\" \\\\ \\{ and \\}"));
                    }
                }
                i += 1;
                continue;
            }
            if c == '{' {
                let brace_col = (i + 1) as u32;
                let content_start = i + 1;
                let content_end = self.find_hole_end(chars, ln, i, brace_col)?;
                if chars[content_start..content_end]
                    .iter()
                    .all(|c| c.is_whitespace())
                {
                    return Err(self
                        .diag(ln, brace_col, "this {} has nothing to show")
                        .with_headline("very empty. much hole.")
                        .with_hint("put an expression inside, or write \\{ for a literal brace"));
                }
                if !lit.is_empty() {
                    segments.push(StrSegment::Lit(std::mem::take(&mut lit)));
                }
                let hole = self.lex_hole(chars, ln, content_start, content_end)?;
                segments.push(StrSegment::Hole(hole));
                i = content_end + 1; // past the closing `}`
                continue;
            }
            // A `}` outside a hole is an ordinary character; `\}` reaches the
            // escape arm above.
            lit.push(c);
            i += 1;
        }
        // Reached end of line with no closing quote (a raw newline can't appear
        // inside `chars`, which is a single physical line).
        Err(self
            .diag(ln, col, "this string never closes")
            .with_headline("very string. much unfinished.")
            .with_hint("add a closing \" on this line"))
    }

    /// Find the `}` that closes the interpolation hole whose `{` is at `open`.
    /// Scans the rest of the physical line, matching nested `{ }` (dict literals)
    /// and skipping over nested string literals so a `}` inside them does not
    /// close the hole. Returns the index of the closing `}`.
    pub(super) fn find_hole_end(
        &self,
        chars: &[char],
        ln: u32,
        open: usize,
        brace_col: u32,
    ) -> Result<usize, Diagnostic> {
        let mut depth = 1usize;
        let mut in_string = false;
        let mut j = open + 1;
        while j < chars.len() {
            let c = chars[j];
            if in_string {
                if c == '\\' {
                    j += 2; // skip the escaped character
                    continue;
                }
                if c == '"' {
                    in_string = false;
                }
            } else if c == '"' {
                in_string = true;
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    return Ok(j);
                }
            }
            j += 1;
        }
        Err(self
            .diag(ln, brace_col, "this {…} interpolation never closes")
            .with_headline("very hole. much open.")
            .with_hint("add a closing } on this line"))
    }

    /// Lex the content of an interpolation hole (`[start, end)`) into its own
    /// token stream, using the same machinery as a normal line so every token
    /// keeps a real source span. The main token buffer and bracket stack are
    /// swapped out and restored around the sub-lex.
    pub(super) fn lex_hole(
        &mut self,
        chars: &[char],
        ln: u32,
        start: usize,
        end: usize,
    ) -> Result<Vec<Token>, Diagnostic> {
        let saved_tokens = std::mem::take(&mut self.tokens);
        let saved_brackets = std::mem::take(&mut self.bracket_stack);
        self.lex_range(chars, ln, start, end)?;
        if let Some(open) = self.bracket_stack.first() {
            let span = *open;
            return Err(self
                .diag(span.line, span.col, "this bracket was never closed")
                .with_headline("very open. much bracket.")
                .with_hint("add the matching closing bracket"));
        }
        let hole = std::mem::replace(&mut self.tokens, saved_tokens);
        self.bracket_stack = saved_brackets;
        Ok(hole)
    }
}
