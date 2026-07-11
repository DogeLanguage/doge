use super::*;

impl Lexer {
    /// Lex an identifier or keyword starting at `start`; also fuses `oh no`.
    /// Returns the index just past the consumed token.
    pub(super) fn lex_word(&mut self, chars: &[char], ln: u32, start: usize) -> usize {
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
    pub(super) fn lex_number(
        &mut self,
        chars: &[char],
        ln: u32,
        start: usize,
    ) -> Result<usize, Diagnostic> {
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

    /// Lex a single operator or punctuation token. Longer operators are checked
    /// before their shorter prefixes (`**=` before `**` before `*`).
    pub(super) fn lex_operator(
        &mut self,
        chars: &[char],
        ln: u32,
        i: usize,
        col: u32,
    ) -> Result<usize, Diagnostic> {
        use crate::ast::BinOp;

        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let third = chars.get(i + 2).copied();

        // Three-character augmented assignments first.
        let three = match (c, next, third) {
            ('*', Some('*'), Some('=')) => Some(BinOp::Pow),
            ('/', Some('/'), Some('=')) => Some(BinOp::FloorDiv),
            ('<', Some('<'), Some('=')) => Some(BinOp::Shl),
            ('>', Some('>'), Some('=')) => Some(BinOp::Shr),
            _ => None,
        };
        if let Some(op) = three {
            self.push(TokenKind::AugAssign(op), ln, col);
            return Ok(i + 3);
        }

        // Two-character operators.
        let two = match (c, next) {
            ('*', Some('*')) => Some(TokenKind::StarStar),
            ('/', Some('/')) => Some(TokenKind::SlashSlash),
            ('<', Some('<')) => Some(TokenKind::Shl),
            ('>', Some('>')) => Some(TokenKind::Shr),
            ('=', Some('=')) => Some(TokenKind::EqEq),
            ('!', Some('=')) => Some(TokenKind::NotEq),
            ('<', Some('=')) => Some(TokenKind::LtEq),
            ('>', Some('=')) => Some(TokenKind::GtEq),
            ('+', Some('=')) => Some(TokenKind::AugAssign(BinOp::Add)),
            ('-', Some('=')) => Some(TokenKind::AugAssign(BinOp::Sub)),
            ('*', Some('=')) => Some(TokenKind::AugAssign(BinOp::Mul)),
            ('/', Some('=')) => Some(TokenKind::AugAssign(BinOp::Div)),
            ('%', Some('=')) => Some(TokenKind::AugAssign(BinOp::Rem)),
            ('&', Some('=')) => Some(TokenKind::AugAssign(BinOp::BitAnd)),
            ('|', Some('=')) => Some(TokenKind::AugAssign(BinOp::BitOr)),
            ('^', Some('=')) => Some(TokenKind::AugAssign(BinOp::BitXor)),
            _ => None,
        };
        if let Some(kind) = two {
            self.push(kind, ln, col);
            return Ok(i + 2);
        }

        let kind = match c {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '&' => TokenKind::Amp,
            '|' => TokenKind::Pipe,
            '^' => TokenKind::Caret,
            '~' => TokenKind::Tilde,
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
}
