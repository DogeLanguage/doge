use num_bigint::BigInt;

use crate::ast::BinOp;

/// A 1-based source position pointing at the first character of a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

/// A lexed token: its kind plus where it started.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// One piece of an interpolated string literal (`"a {b} c"`): either literal
/// text or a `{…}` hole already lexed into its own token stream (with real
/// source spans, so downstream diagnostics point at the right column).
#[derive(Debug, Clone, PartialEq)]
pub enum StrSegment {
    Lit(String),
    Hole(Vec<Token>),
}

/// Every kind of token Doge source can produce.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Pls,
    Bork,
    Bonk,
    Bark,
    Wow,
    Such,
    Much,
    Many,
    So,
    Very,
    /// `super` — call a method inherited from the enclosing class's parent.
    Super,
    /// The fused `oh no` compound keyword
    OhNo,

    If,
    Elif,
    Else,
    For,
    While,
    In,
    Return,
    Continue,
    And,
    Or,
    Not,
    True,
    False,
    None,

    Def,
    Class,
    Amaze,

    Ident(String),
    /// An integer literal at full width: `Int` is arbitrary precision, so a literal
    /// larger than `i64` must survive to codegen intact.
    Int(BigInt),
    Float(f64),
    Str(String),
    /// A string literal containing at least one `{…}` interpolation hole.
    StrInterp(Vec<StrSegment>),

    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    SlashSlash,
    Percent,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Eq,
    /// A compound assignment `op=`, e.g. `+=`, `//=`, `<<=`. Carries the binary
    /// operator applied before the store.
    AugAssign(BinOp),
    Colon,
    Bang,
    Comma,
    Dot,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,

    Newline,
    Indent,
    Dedent,
    Eof,
}

impl TokenKind {
    /// A short human-readable name for this kind, used when a diagnostic needs
    /// to name the token it did not expect.
    pub fn describe(&self) -> String {
        match self {
            // Keyword tokens carry their spelling in the KEYWORDS table, so the
            // spelling lives in one place; a new keyword variant makes this match
            // non-exhaustive until it is added here too.
            TokenKind::Pls
            | TokenKind::Bork
            | TokenKind::Bonk
            | TokenKind::Bark
            | TokenKind::Wow
            | TokenKind::Such
            | TokenKind::Much
            | TokenKind::Many
            | TokenKind::So
            | TokenKind::Very
            | TokenKind::Super
            | TokenKind::If
            | TokenKind::Elif
            | TokenKind::Else
            | TokenKind::For
            | TokenKind::While
            | TokenKind::In
            | TokenKind::Return
            | TokenKind::Continue
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Not
            | TokenKind::True
            | TokenKind::False
            | TokenKind::None
            | TokenKind::Def
            | TokenKind::Class
            | TokenKind::Amaze => crate::keywords::keyword_spelling(self)
                .expect("compiler bug: keyword token missing from KEYWORDS table")
                .into(),
            TokenKind::OhNo => "oh no".into(),
            TokenKind::Ident(name) => format!("name '{name}'"),
            TokenKind::Int(n) => format!("the number {n}"),
            TokenKind::Float(f) => format!("the number {f}"),
            TokenKind::Str(_) | TokenKind::StrInterp(_) => "a string".into(),
            TokenKind::Plus => "+".into(),
            TokenKind::Minus => "-".into(),
            TokenKind::Star => "*".into(),
            TokenKind::StarStar => "**".into(),
            TokenKind::Slash => "/".into(),
            TokenKind::SlashSlash => "//".into(),
            TokenKind::Percent => "%".into(),
            TokenKind::Amp => "&".into(),
            TokenKind::Pipe => "|".into(),
            TokenKind::Caret => "^".into(),
            TokenKind::Tilde => "~".into(),
            TokenKind::Shl => "<<".into(),
            TokenKind::Shr => ">>".into(),
            TokenKind::EqEq => "==".into(),
            TokenKind::NotEq => "!=".into(),
            TokenKind::Lt => "<".into(),
            TokenKind::LtEq => "<=".into(),
            TokenKind::Gt => ">".into(),
            TokenKind::GtEq => ">=".into(),
            TokenKind::Eq => "=".into(),
            TokenKind::AugAssign(op) => format!("{}=", op.symbol()),
            TokenKind::Colon => ":".into(),
            TokenKind::Bang => "!".into(),
            TokenKind::Comma => ",".into(),
            TokenKind::Dot => ".".into(),
            TokenKind::LParen => "(".into(),
            TokenKind::RParen => ")".into(),
            TokenKind::LBracket => "[".into(),
            TokenKind::RBracket => "]".into(),
            TokenKind::LBrace => "{".into(),
            TokenKind::RBrace => "}".into(),
            TokenKind::Newline => "the end of the line".into(),
            TokenKind::Indent => "more indentation".into(),
            TokenKind::Dedent => "less indentation".into(),
            TokenKind::Eof => "the end of the script".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keywords::{keyword_spelling, lookup, KEYWORDS};

    #[test]
    fn every_keyword_round_trips_through_lookup_and_describe() {
        for (spelling, kind) in KEYWORDS {
            assert_eq!(lookup(spelling).as_ref(), Some(kind));
            assert_eq!(keyword_spelling(kind), Some(*spelling));
            assert_eq!(kind.describe(), *spelling);
        }
    }

    #[test]
    fn non_keyword_tokens_have_no_keyword_spelling() {
        assert_eq!(keyword_spelling(&TokenKind::OhNo), None);
        assert_eq!(keyword_spelling(&TokenKind::Plus), None);
        assert_eq!(keyword_spelling(&TokenKind::Eof), None);
    }

    #[test]
    fn ohno_describes_as_two_words() {
        assert_eq!(TokenKind::OhNo.describe(), "oh no");
    }
}
