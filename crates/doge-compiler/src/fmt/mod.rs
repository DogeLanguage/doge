//! The `doge fmt` formatter. It reflows the token stream to canonical style —
//! four-space indentation, normalized spacing around operators and punctuation —
//! while preserving every `#` comment. It works over tokens rather than the AST
//! because the AST does not carry comments, and it never invents or removes line
//! breaks: a bracketed expression the author split across lines stays split, and
//! one written on a single line stays single. The only structural change is
//! whitespace.
//!
//! A safety net re-lexes the result and refuses to emit anything whose token
//! stream or comments differ from the input, so formatting can never change what
//! a script means.

use crate::diagnostics::{split_source_lines, Diagnostic};
use crate::lexer::{self, Comment};
use crate::token::{StrSegment, Token, TokenKind};

#[cfg(test)]
mod tests;

/// One unit of indentation.
const INDENT: &str = "    ";

/// Format `source`, or return the parser's diagnostic if it does not parse.
pub(crate) fn format(path: &str, source: &str) -> Result<String, Diagnostic> {
    // Refuse to format code that does not parse — reflowing broken source could
    // only make its diagnostics harder to read.
    crate::parser::parse(path, source)?;

    let (tokens, comments) = lexer::lex_with_comments(path, source)?;
    let src_lines = split_source_lines(source);
    let formatted = render(&tokens, &comments, &src_lines);

    // The formatter must never change the token stream or drop a comment. If it
    // did, that is a compiler bug — report it as one rather than emit a script
    // that means something different from what the author wrote.
    if let Ok((new_tokens, new_comments)) = lexer::lex_with_comments(path, &formatted) {
        let tokens_match = tokens.len() == new_tokens.len()
            && tokens
                .iter()
                .zip(&new_tokens)
                .all(|(a, b)| same_kind(&a.kind, &b.kind));
        let comments_match = comments
            .iter()
            .map(|c| &c.text)
            .eq(new_comments.iter().map(|c| &c.text));
        if tokens_match && comments_match {
            return Ok(formatted);
        }
    }
    Err(compiler_bug(path, &src_lines))
}

/// One physical output line built from code tokens (a logical statement split
/// across brackets becomes several of these).
struct CodeLine {
    /// The source line these tokens came from.
    src_line: u32,
    /// Indentation, in [`INDENT`] units.
    indent: usize,
    /// The block-nesting depth of the enclosing statement (used to place
    /// own-line comments; equal for every physical line of one statement).
    depth: usize,
    text: String,
    /// A `#` comment that trailed these tokens on the same source line.
    trailing: Option<String>,
}

fn render(tokens: &[Token], comments: &[Comment], src_lines: &[String]) -> String {
    let mut code_lines = build_code_lines(tokens, src_lines, comments);

    let code_src: std::collections::HashSet<u32> = code_lines.iter().map(|c| c.src_line).collect();
    for comment in comments {
        if let Some(line) = code_lines.iter_mut().find(|l| l.src_line == comment.line) {
            line.trailing = Some(comment.text.clone());
        }
    }

    // Merge code lines and own-line comments into one source-ordered list of
    // (source line, indent units, rendered body).
    let mut items: Vec<(u32, usize, String)> = Vec::new();
    for line in &code_lines {
        let body = match &line.trailing {
            Some(comment) => format!("{}  #{}", line.text, comment),
            None => line.text.clone(),
        };
        items.push((line.src_line, line.indent, body));
    }
    for comment in comments {
        if code_src.contains(&comment.line) {
            continue; // trailing comment, already attached above
        }
        let depth = own_line_comment_depth(comment, &code_lines);
        items.push((comment.line, depth, format!("#{}", comment.text)));
    }
    items.sort_by_key(|(line, _, _)| *line);

    // Assemble, capping runs of blank source lines at a single blank line (and so
    // trimming any leading or trailing blanks, since nothing precedes the first
    // item or follows the last).
    let mut out = String::new();
    let mut prev_line: Option<u32> = None;
    for (src_line, indent, body) in &items {
        if let Some(prev) = prev_line {
            if *src_line > prev + 1 {
                out.push('\n');
            }
        }
        out.push_str(&INDENT.repeat(*indent));
        out.push_str(body);
        out.push('\n');
        prev_line = Some(*src_line);
    }
    out
}

fn build_code_lines(tokens: &[Token], src_lines: &[String], comments: &[Comment]) -> Vec<CodeLine> {
    let mut lines: Vec<CodeLine> = Vec::new();

    let mut block_depth = 0usize;
    // Bracket nesting as tokens are emitted; its length is the depth, and the top
    // tells a `:` whether it is a slice colon (`[`) or a dict colon (`{`).
    let mut brackets: Vec<char> = Vec::new();

    let mut buf = String::new();
    let mut line_src = 0u32;
    let mut line_indent = 0usize;
    // Whether the physical line being built is a bracket continuation of an
    // earlier line of the same statement (not the statement's first line).
    let mut is_continuation = false;

    // Carried across the physical lines of one statement; reset at each Newline.
    let mut prev_kind: Option<TokenKind> = None;
    let mut prev_unary = false;

    for (k, tok) in tokens.iter().enumerate() {
        match &tok.kind {
            TokenKind::Indent => block_depth += 1,
            TokenKind::Dedent => block_depth = block_depth.saturating_sub(1),
            TokenKind::Newline | TokenKind::Eof => {
                flush(&mut lines, &mut buf, line_src, line_indent, block_depth);
                prev_kind = None;
                prev_unary = false;
                is_continuation = false;
            }
            _ => {
                let line_break = !buf.is_empty() && tok.span.line != line_src;
                if line_break {
                    flush(&mut lines, &mut buf, line_src, line_indent, block_depth);
                    is_continuation = true;
                }
                if buf.is_empty() {
                    line_src = tok.span.line;
                    line_indent = if is_continuation {
                        // Continuation lines sit under the brackets still open from
                        // earlier lines; a line that opens with a closer dedents to
                        // the level of its opener.
                        let first_is_closer = matches!(
                            tok.kind,
                            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace
                        );
                        let open = if first_is_closer {
                            brackets.len().saturating_sub(1)
                        } else {
                            brackets.len()
                        };
                        block_depth + open
                    } else {
                        block_depth
                    };
                }

                let text = render_token_text(tokens, k, src_lines, comments);
                if buf.is_empty() {
                    buf.push_str(&text);
                } else {
                    buf.push_str(sep(
                        prev_kind
                            .as_ref()
                            .expect("compiler bug: non-empty line has a prev token"),
                        &tok.kind,
                        &brackets,
                        prev_unary,
                    ));
                    buf.push_str(&text);
                }

                match tok.kind {
                    TokenKind::LParen => brackets.push('('),
                    TokenKind::LBracket => brackets.push('['),
                    TokenKind::LBrace => brackets.push('{'),
                    TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                        brackets.pop();
                    }
                    _ => {}
                }

                prev_unary = is_unary(&tok.kind, prev_kind.as_ref());
                prev_kind = Some(tok.kind.clone());
            }
        }
    }
    lines
}

fn flush(lines: &mut Vec<CodeLine>, buf: &mut String, src_line: u32, indent: usize, depth: usize) {
    if !buf.is_empty() {
        lines.push(CodeLine {
            src_line,
            indent,
            depth,
            text: std::mem::take(buf),
            trailing: None,
        });
    }
}

/// The rendered text of one token: identifiers and literals keep their exact
/// source spelling; every operator, keyword, and punctuation mark renders from
/// the one keyword/spelling table via [`TokenKind::describe`].
fn render_token_text(
    tokens: &[Token],
    k: usize,
    src_lines: &[String],
    comments: &[Comment],
) -> String {
    match &tokens[k].kind {
        TokenKind::Ident(name) => name.clone(),
        TokenKind::Int(_) | TokenKind::Float(_) | TokenKind::Str(_) | TokenKind::StrInterp(_) => {
            slice_literal(tokens, k, src_lines, comments)
        }
        other => other.describe(),
    }
}

/// Slice a literal verbatim from its source line — its decoded value (an escaped
/// string, a parsed float) would not round-trip to the same spelling. The text
/// runs from the token's column to the next token on the line, or to a trailing
/// comment, or to the end of the line.
fn slice_literal(tokens: &[Token], k: usize, src_lines: &[String], comments: &[Comment]) -> String {
    let tok = &tokens[k];
    let line = tok.span.line;
    let chars: Vec<char> = src_lines
        .get((line as usize).saturating_sub(1))
        .map(|l| l.chars().collect())
        .unwrap_or_default();
    let start = tok.span.col.saturating_sub(1) as usize;

    let mut end = chars.len();
    if let Some(next) = tokens.get(k + 1) {
        if next.span.line == line && !matches!(next.kind, TokenKind::Eof) {
            end = end.min(next.span.col.saturating_sub(1) as usize);
        }
    }
    if let Some(comment) = comments.iter().find(|c| c.line == line) {
        end = end.min(comment.col.saturating_sub(1) as usize);
    }
    let end = end.clamp(start, chars.len());
    chars[start..end]
        .iter()
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Whether a space separates two adjacent tokens on the same line. The default is
/// a single space (binary operators, keywords, after a comma); the exceptions are
/// the tight forms — member access, call/index parentheses, unary operators,
/// bracket interiors, and slice colons.
fn sep(prev: &TokenKind, cur: &TokenKind, brackets: &[char], prev_unary: bool) -> &'static str {
    if matches!(prev, TokenKind::Dot) || matches!(cur, TokenKind::Dot) {
        return "";
    }
    if matches!(
        cur,
        TokenKind::Comma
            | TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
            | TokenKind::Colon
            | TokenKind::Bang
    ) {
        return "";
    }
    if matches!(
        prev,
        TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace
    ) {
        return "";
    }
    if prev_unary {
        return "";
    }
    if matches!(cur, TokenKind::LParen | TokenKind::LBracket) && is_value(prev) {
        return "";
    }
    if matches!(prev, TokenKind::Colon) && brackets.last() == Some(&'[') {
        return "";
    }
    " "
}

/// Whether a `-` or `~` at this position is a unary (prefix) operator: `~` always
/// is, and `-` is unless it follows a value (making it subtraction).
fn is_unary(kind: &TokenKind, prev: Option<&TokenKind>) -> bool {
    match kind {
        TokenKind::Tilde => true,
        TokenKind::Minus => !prev.map(is_value).unwrap_or(false),
        _ => false,
    }
}

/// Whether a token can end an expression — so a following `(`/`[` is a call/index
/// and a following `-` is subtraction.
fn is_value(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident(_)
            | TokenKind::Int(_)
            | TokenKind::Float(_)
            | TokenKind::Str(_)
            | TokenKind::StrInterp(_)
            | TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
            | TokenKind::True
            | TokenKind::False
            | TokenKind::None
            | TokenKind::Super
    )
}

/// The indentation for an own-line comment: its original indentation snapped to
/// the four-space grid, clamped to the block depths of the code lines that
/// surround it, so a comment keeps sitting with the code it was written against.
fn own_line_comment_depth(comment: &Comment, code_lines: &[CodeLine]) -> usize {
    let prev = code_lines
        .iter()
        .rfind(|l| l.src_line < comment.line)
        .map(|l| l.depth);
    let next = code_lines
        .iter()
        .find(|l| l.src_line > comment.line)
        .map(|l| l.depth);
    let (lo, hi) = match (prev, next) {
        (Some(p), Some(n)) => (p.min(n), p.max(n)),
        (Some(d), None) | (None, Some(d)) => (d, d),
        (None, None) => (0, 0),
    };
    let width = comment.col.saturating_sub(1) as usize;
    let snapped = (width + INDENT.len() / 2) / INDENT.len();
    snapped.clamp(lo, hi)
}

/// Compare two token kinds ignoring source positions. Only [`TokenKind::StrInterp`]
/// needs special handling: it carries the lexed tokens of its `{…}` holes, whose
/// spans legitimately shift when reflowing adds or removes lines, so the holes are
/// compared by their inner kinds rather than by `==`.
fn same_kind(a: &TokenKind, b: &TokenKind) -> bool {
    match (a, b) {
        (TokenKind::StrInterp(sa), TokenKind::StrInterp(sb)) => {
            sa.len() == sb.len() && sa.iter().zip(sb).all(|(x, y)| same_segment(x, y))
        }
        _ => a == b,
    }
}

fn same_segment(a: &StrSegment, b: &StrSegment) -> bool {
    match (a, b) {
        (StrSegment::Lit(x), StrSegment::Lit(y)) => x == y,
        (StrSegment::Hole(x), StrSegment::Hole(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_kind(&p.kind, &q.kind))
        }
        _ => false,
    }
}

fn compiler_bug(path: &str, src_lines: &[String]) -> Diagnostic {
    let first = src_lines.first().cloned().unwrap_or_default();
    Diagnostic::new(
        path,
        1,
        1,
        first,
        "the formatter changed this script's meaning — this is a doge bug, not your code",
    )
    .with_headline("very bug. much sorry.")
    .with_hint("pls report it at https://github.com/DogeLanguage/doge/issues")
}
