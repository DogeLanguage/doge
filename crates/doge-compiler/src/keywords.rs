use crate::token::TokenKind;

/// Map a bare word to its keyword [`TokenKind`]
pub fn lookup(word: &str) -> Option<TokenKind> {
    let kind = match word {
        // Doge keywords
        "pls" => TokenKind::Pls,
        "bork" => TokenKind::Bork,
        "bark" => TokenKind::Bark,
        "wow" => TokenKind::Wow,
        "such" => TokenKind::Such,
        "much" => TokenKind::Much,
        "many" => TokenKind::Many,
        "so" => TokenKind::So,
        "very" => TokenKind::Very,

        // Universal keywords
        "if" => TokenKind::If,
        "elif" => TokenKind::Elif,
        "else" => TokenKind::Else,
        "for" => TokenKind::For,
        "while" => TokenKind::While,
        "in" => TokenKind::In,
        "return" => TokenKind::Return,
        "continue" => TokenKind::Continue,
        "and" => TokenKind::And,
        "or" => TokenKind::Or,
        "not" => TokenKind::Not,
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "none" => TokenKind::None,

        // Reserved words lexed as keywords so the parser can
        // greet Python muscle memory with a friendly hint instead of a vague
        // "unexpected identifier".
        "def" => TokenKind::Def,
        "class" => TokenKind::Class,
        "amaze" => TokenKind::Amaze,

        _ => return Option::None,
    };
    Some(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_words_map_to_keywords() {
        assert!(matches!(lookup("such"), Some(TokenKind::Such)));
        assert!(matches!(lookup("wow"), Some(TokenKind::Wow)));
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
