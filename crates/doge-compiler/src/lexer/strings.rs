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
                    'r' => lit.push('\r'),
                    '0' => lit.push('\0'),
                    '"' => lit.push('"'),
                    '\\' => lit.push('\\'),
                    '{' => lit.push('{'),
                    '}' => lit.push('}'),
                    'x' => {
                        let ch = self.lex_hex_escape(chars, ln, esc_col, i)?;
                        lit.push(ch);
                        i += 3; // past `x` and the two hex digits
                        continue;
                    }
                    'u' => {
                        let (ch, consumed) = self.lex_unicode_escape(chars, ln, esc_col, i)?;
                        lit.push(ch);
                        i += consumed; // past `u{…}`
                        continue;
                    }
                    other => {
                        return Err(self
                            .diag(
                                ln,
                                esc_col,
                                format!("'\\{other}' is not an escape doge knows"),
                            )
                            .with_hint(
                                "known escapes are \\n \\t \\r \\0 \\\" \\\\ \\{ \\} \\xNN and \\u{…}",
                            ));
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

    /// Decode a `\xNN` escape: exactly two hex digits naming an ASCII scalar
    /// (0x00–0x7F). `x_idx` is the index of the `x`; `esc_col` the 1-based column
    /// of the backslash for the caret. Higher code points go through `\u{…}`.
    fn lex_hex_escape(
        &self,
        chars: &[char],
        ln: u32,
        esc_col: u32,
        x_idx: usize,
    ) -> Result<char, Diagnostic> {
        let bad = |me: &Self| {
            me.diag(ln, esc_col, "this \\x escape is not two hex digits 00–7f")
                .with_headline("very hex. much confuse.")
                .with_hint(
                    "an \\x escape wants two hex digits 00–7f, like \\x0d — for higher code points use \\u{…}",
                )
        };
        let (Some(&hi), Some(&lo)) = (chars.get(x_idx + 1), chars.get(x_idx + 2)) else {
            return Err(bad(self));
        };
        let (Some(hi), Some(lo)) = (hi.to_digit(16), lo.to_digit(16)) else {
            return Err(bad(self));
        };
        let value = hi * 16 + lo;
        if value > 0x7f {
            return Err(bad(self));
        }
        Ok(char::from(value as u8))
    }

    /// Decode a `\u{…}` escape: 1–6 hex digits naming a valid Unicode scalar.
    /// `u_idx` is the index of the `u`. Returns the char and how many chars the
    /// `u{…}` span occupies, so the caller can advance past the closing `}`.
    fn lex_unicode_escape(
        &self,
        chars: &[char],
        ln: u32,
        esc_col: u32,
        u_idx: usize,
    ) -> Result<(char, usize), Diagnostic> {
        let bad = |me: &Self| {
            me.diag(ln, esc_col, "this \\u{…} escape names no valid character")
                .with_headline("very unicode. much confuse.")
                .with_hint(
                    "write \\u{…} with 1–6 hex digits naming a real character, like \\u{1f436}",
                )
        };
        if chars.get(u_idx + 1) != Some(&'{') {
            return Err(bad(self));
        }
        let mut j = u_idx + 2;
        let mut digits = String::new();
        while let Some(&c) = chars.get(j) {
            if c == '}' {
                break;
            }
            if !c.is_ascii_hexdigit() {
                return Err(bad(self));
            }
            digits.push(c);
            j += 1;
        }
        if chars.get(j) != Some(&'}') || digits.is_empty() || digits.len() > 6 {
            return Err(bad(self));
        }
        let value = u32::from_str_radix(&digits, 16).map_err(|_| bad(self))?;
        let ch = char::from_u32(value).ok_or_else(|| bad(self))?;
        let consumed = j - u_idx + 1; // `u` through `}` inclusive
        Ok((ch, consumed))
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
        let saved_in_hole = std::mem::replace(&mut self.in_hole, true);
        let result = self.lex_range(chars, ln, start, end);
        self.in_hole = saved_in_hole;
        result?;
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
