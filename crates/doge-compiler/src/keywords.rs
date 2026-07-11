use crate::token::TokenKind;

/// The one keyword table: every bare word that lexes to a keyword token, paired
/// with the token it produces. The lexer's [`lookup`] and diagnostics'
/// [`keyword_spelling`] both read this, so a keyword's spelling lives in exactly
/// one place. The fused `oh no` compound is not here (it is not a bare word).
///
/// Reserved words (`def`/`class`/`amaze`) are lexed as keywords so the parser can
/// greet Python muscle memory with a friendly hint instead of a vague
/// "unexpected identifier".
pub const KEYWORDS: &[(&str, TokenKind)] = &[
    ("pls", TokenKind::Pls),
    ("bork", TokenKind::Bork),
    ("bonk", TokenKind::Bonk),
    ("bark", TokenKind::Bark),
    ("wow", TokenKind::Wow),
    ("such", TokenKind::Such),
    ("much", TokenKind::Much),
    ("many", TokenKind::Many),
    ("so", TokenKind::So),
    ("very", TokenKind::Very),
    ("if", TokenKind::If),
    ("elif", TokenKind::Elif),
    ("else", TokenKind::Else),
    ("for", TokenKind::For),
    ("while", TokenKind::While),
    ("in", TokenKind::In),
    ("return", TokenKind::Return),
    ("continue", TokenKind::Continue),
    ("and", TokenKind::And),
    ("or", TokenKind::Or),
    ("not", TokenKind::Not),
    ("true", TokenKind::True),
    ("false", TokenKind::False),
    ("none", TokenKind::None),
    ("def", TokenKind::Def),
    ("class", TokenKind::Class),
    ("amaze", TokenKind::Amaze),
];

/// Map a bare word to its keyword [`TokenKind`], or `None` if it is an ordinary
/// identifier.
pub fn lookup(word: &str) -> Option<TokenKind> {
    KEYWORDS
        .iter()
        .find(|(spelling, _)| *spelling == word)
        .map(|(_, kind)| kind.clone())
}

/// The bare-word spelling of a keyword token, for diagnostics. `None` for any
/// token that is not a keyword (operators, literals, the fused `oh no`).
pub fn keyword_spelling(kind: &TokenKind) -> Option<&'static str> {
    KEYWORDS
        .iter()
        .find(|(_, k)| k == kind)
        .map(|(spelling, _)| *spelling)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_words_map_to_keywords() {
        assert!(matches!(lookup("such"), Some(TokenKind::Such)));
        assert!(matches!(lookup("wow"), Some(TokenKind::Wow)));
        assert!(matches!(lookup("bonk"), Some(TokenKind::Bonk)));
        assert!(matches!(lookup("if"), Some(TokenKind::If)));
        assert!(matches!(lookup("def"), Some(TokenKind::Def)));
    }

    #[test]
    fn unknown_words_are_identifiers() {
        assert!(lookup("kabosu").is_none());
        assert!(lookup("greet").is_none());
        // `oh` and `no` are NOT keywords on their own — the lexer fuses them.
        assert!(lookup("oh").is_none());
        assert!(lookup("no").is_none());
    }
}
