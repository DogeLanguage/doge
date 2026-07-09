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
    /// The fused `oh no` compound keyword
    OhNo,

    // Universal keywords
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

    // Reserved words
    Def,
    Class,
    Amaze,

    // --- Literals and identifiers ---
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),

    // --- Operators ---
    Plus,
    Minus,
    Star,
    Slash,
    SlashSlash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Eq,
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

    // --- Structural (synthesized by the lexer) ---
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
            TokenKind::Pls => "pls".into(),
            TokenKind::Bork => "bork".into(),
            TokenKind::Bonk => "bonk".into(),
            TokenKind::Bark => "bark".into(),
            TokenKind::Wow => "wow".into(),
            TokenKind::Such => "such".into(),
            TokenKind::Much => "much".into(),
            TokenKind::Many => "many".into(),
            TokenKind::So => "so".into(),
            TokenKind::Very => "very".into(),
            TokenKind::OhNo => "oh no".into(),
            TokenKind::If => "if".into(),
            TokenKind::Elif => "elif".into(),
            TokenKind::Else => "else".into(),
            TokenKind::For => "for".into(),
            TokenKind::While => "while".into(),
            TokenKind::In => "in".into(),
            TokenKind::Return => "return".into(),
            TokenKind::Continue => "continue".into(),
            TokenKind::And => "and".into(),
            TokenKind::Or => "or".into(),
            TokenKind::Not => "not".into(),
            TokenKind::True => "true".into(),
            TokenKind::False => "false".into(),
            TokenKind::None => "none".into(),
            TokenKind::Def => "def".into(),
            TokenKind::Class => "class".into(),
            TokenKind::Amaze => "amaze".into(),
            TokenKind::Ident(name) => format!("name '{name}'"),
            TokenKind::Int(n) => format!("the number {n}"),
            TokenKind::Float(f) => format!("the number {f}"),
            TokenKind::Str(_) => "a string".into(),
            TokenKind::Plus => "+".into(),
            TokenKind::Minus => "-".into(),
            TokenKind::Star => "*".into(),
            TokenKind::Slash => "/".into(),
            TokenKind::SlashSlash => "//".into(),
            TokenKind::Percent => "%".into(),
            TokenKind::EqEq => "==".into(),
            TokenKind::NotEq => "!=".into(),
            TokenKind::Lt => "<".into(),
            TokenKind::LtEq => "<=".into(),
            TokenKind::Gt => ">".into(),
            TokenKind::GtEq => ">=".into(),
            TokenKind::Eq => "=".into(),
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
