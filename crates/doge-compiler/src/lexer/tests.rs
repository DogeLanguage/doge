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
fn plain_string_stays_a_str() {
    let toks = kinds("bark \"much hello\"\n");
    assert_eq!(toks[1], TokenKind::Str("much hello".into()));
}

#[test]
fn interpolated_string_splits_into_segments() {
    let toks = kinds("bark \"hi {name}!\"\n");
    let TokenKind::StrInterp(segments) = &toks[1] else {
        panic!("expected StrInterp, got {:?}", toks[1]);
    };
    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0], StrSegment::Lit("hi ".into()));
    let StrSegment::Hole(hole) = &segments[1] else {
        panic!("expected a hole");
    };
    assert_eq!(hole[0].kind, TokenKind::Ident("name".into()));
    // The hole token keeps its real column (the `n` of `name`).
    assert_eq!(hole[0].span.col, 11);
    assert_eq!(segments[2], StrSegment::Lit("!".into()));
}

#[test]
fn hole_can_hold_a_nested_string() {
    let toks = kinds("bark \"{f(\"x\")}\"\n");
    let TokenKind::StrInterp(segments) = &toks[1] else {
        panic!("expected StrInterp");
    };
    let StrSegment::Hole(hole) = &segments[0] else {
        panic!("expected a hole");
    };
    // f ( "x" ) Eof-free: the sub-lex produced these kinds.
    assert_eq!(hole[0].kind, TokenKind::Ident("f".into()));
    assert_eq!(hole[1].kind, TokenKind::LParen);
    assert_eq!(hole[2].kind, TokenKind::Str("x".into()));
    assert_eq!(hole[3].kind, TokenKind::RParen);
}

#[test]
fn escaped_brace_is_a_literal_not_a_hole() {
    let toks = kinds("bark \"\\{name}\"\n");
    assert_eq!(toks[1], TokenKind::Str("{name}".into()));
}

#[test]
fn bare_close_brace_is_literal() {
    let toks = kinds("bark \"a } b\"\n");
    assert_eq!(toks[1], TokenKind::Str("a } b".into()));
}

#[test]
fn unclosed_hole_is_an_error() {
    let err = lex("test.doge", "bark \"oops {1 + 2\"\n").unwrap_err();
    assert_eq!(err.headline, "very hole. much open.");
}

#[test]
fn empty_hole_is_an_error() {
    let err = lex("test.doge", "bark \"empty {}\"\n").unwrap_err();
    assert_eq!(err.headline, "very empty. much hole.");
    let err = lex("test.doge", "bark \"empty {   }\"\n").unwrap_err();
    assert_eq!(err.headline, "very empty. much hole.");
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
