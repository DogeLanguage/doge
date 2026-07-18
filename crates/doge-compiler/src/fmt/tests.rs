use super::format;

/// Format a snippet, asserting it parses and formats cleanly.
fn fmt(source: &str) -> String {
    format("test.doge", source).expect("snippet should format")
}

#[test]
fn normalizes_operator_and_comma_spacing() {
    let out = fmt("such xs=[1,2 ,  3]\nbark  1+2*3\nwow\n");
    assert_eq!(out, "such xs = [1, 2, 3]\nbark 1 + 2 * 3\nwow\n");
}

#[test]
fn keeps_calls_indexes_and_members_tight() {
    let out = fmt("bark greet ( \"kabosu\" )\nbark xs [ 0 ]\nbark self . name\nwow\n");
    assert_eq!(
        out,
        "bark greet(\"kabosu\")\nbark xs[0]\nbark self.name\nwow\n"
    );
}

#[test]
fn unary_minus_and_tilde_bind_tight() {
    let out = fmt("bark - 1\nbark 2 ** - 1\nbark ~ 0\nbark a - b\nwow\n");
    assert_eq!(out, "bark -1\nbark 2 ** -1\nbark ~0\nbark a - b\nwow\n");
}

#[test]
fn slice_colons_are_tight_dict_colons_space_after() {
    let out = fmt("bark xs[1 : 3]\nbark xs[ :: -1]\nsuch d = {\"a\" : 1}\nwow\n");
    assert_eq!(
        out,
        "bark xs[1:3]\nbark xs[::-1]\nsuch d = {\"a\": 1}\nwow\n"
    );
}

#[test]
fn reindents_blocks_to_four_spaces() {
    let out = fmt("if x:\n  if y:\n        bark 1\nwow\n");
    assert_eq!(out, "if x:\n    if y:\n        bark 1\nwow\n");
}

#[test]
fn preserves_own_line_and_trailing_comments() {
    let out = fmt("# top note\nbark 1  # after\nwow\n");
    assert_eq!(out, "# top note\nbark 1  # after\nwow\n");
}

#[test]
fn own_line_comment_keeps_its_block_indentation() {
    let out = fmt("if x:\n    # inside\n    bark 1\nwow\n");
    assert_eq!(out, "if x:\n    # inside\n    bark 1\nwow\n");
}

#[test]
fn caps_blank_lines_and_trims_edges() {
    let out = fmt("\n\nbark 1\n\n\n\nbark 2\n\n\nwow\n");
    assert_eq!(out, "bark 1\n\nbark 2\n\nwow\n");
}

#[test]
fn preserves_multi_line_bracket_layout() {
    let source = "such nums = [\n    10,\n    20,\n]\nwow\n";
    assert_eq!(fmt(source), source);
}

#[test]
fn reindents_a_scrambled_multi_line_bracket() {
    let out = fmt("such nums = [\n1,\n  2,\n     ]\nwow\n");
    assert_eq!(out, "such nums = [\n    1,\n    2,\n]\nwow\n");
}

#[test]
fn preserves_literal_spelling() {
    let out = fmt("bark 3.0\nbark \"a\\tb\"\nwow\n");
    assert_eq!(out, "bark 3.0\nbark \"a\\tb\"\nwow\n");
}

#[test]
fn interpolation_is_left_verbatim() {
    let out = fmt("bark \"hi {name}, {1 + 2}\"\nwow\n");
    assert_eq!(out, "bark \"hi {name}, {1 + 2}\"\nwow\n");
}

#[test]
fn is_idempotent() {
    let messy = "such f much a,b:\n  return a+b*2\nwow\nbark f(1,2)\nwow\n";
    let once = fmt(messy);
    assert_eq!(fmt(&once), once, "formatting a formatted script is a no-op");
}

#[test]
fn oh_no_binding_stays_tight() {
    let out = fmt("pls\n    bonk 1\noh no err !\n    bark err\nwow\n");
    assert_eq!(out, "pls\n    bonk 1\noh no err!\n    bark err\nwow\n");
}

#[test]
fn refuses_to_format_unparseable_source() {
    assert!(format("test.doge", "such =\nwow\n").is_err());
}

#[test]
fn interpolation_survives_line_shifts() {
    // Collapsing blank lines shifts an interpolated string to a new line; the
    // hole tokens' spans change but the token stream is unchanged, so the safety
    // net must not trip.
    let out = fmt("bark 1\n\n\n\nbark \"hi {name}, {1 + 2}\"\nwow\n");
    assert_eq!(out, "bark 1\n\nbark \"hi {name}, {1 + 2}\"\nwow\n");
}
