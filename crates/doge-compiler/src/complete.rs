//! Completion for the language server: given a cursor position in a `.doge`
//! buffer, the candidate names to offer. Every candidate is read from the same
//! single-source tables the checker and codegen use — [`KEYWORDS`], [`BUILTINS`],
//! and the stdlib [`MODULES`] — plus in-scope names computed from the parsed AST,
//! so completion never drifts from what the compiler actually accepts.
//!
//! Completion runs on whatever the editor has in the buffer, which is often
//! mid-edit and does not parse. The AST path is used when the buffer parses; when
//! it does not, a best-effort token scan still surfaces declared names so
//! completion keeps working while the user types. This token scan is a resilience
//! fallback, not a second definition of the language — the binding rules live in
//! the AST walker ([`hoisted_names`]/[`child_funcdefs`]).

use std::collections::HashSet;

use crate::ast::{hoisted_names, Params, Stmt};
use crate::builtins::BUILTINS;
use crate::keywords::KEYWORDS;
use crate::stdlib::{self, MODULES};
use crate::token::{Span, TokenKind};

/// One completion candidate: the text to insert and what kind of thing it is
/// (so the editor can show the right icon).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
}

/// The category of a [`Completion`], mapped to an editor icon by the language
/// server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Keyword,
    Builtin,
    Variable,
    Function,
    Module,
    Member,
}

/// The completion candidates offered at (`line`, `col`) — both 1-based, matching
/// the compiler's [`Span`] convention — in `source` (named `path` for lexing).
pub fn complete(path: &str, source: &str, line: u32, col: u32) -> Vec<Completion> {
    match cursor_context(source, line, col) {
        Context::Member(base) => member_completions(path, source, &base),
        Context::Import => module_name_completions(),
        Context::General => general_completions(path, source, line, col),
    }
}

/// What the text just before the cursor tells us to complete.
enum Context {
    /// `base.<partial>` — the members of `base`.
    Member(String),
    /// Just after `so ` — a module name.
    Import,
    /// Anything else — keywords, builtins, and in-scope names.
    General,
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Read the current line up to the cursor and classify what is being typed.
fn cursor_context(source: &str, line: u32, col: u32) -> Context {
    let line_text = source
        .split('\n')
        .nth((line as usize).saturating_sub(1))
        .unwrap_or("");
    let chars: Vec<char> = line_text.chars().collect();
    let caret = (col.saturating_sub(1) as usize).min(chars.len());
    let prefix = &chars[..caret];

    let mut word = caret;
    while word > 0 && is_ident_char(prefix[word - 1]) {
        word -= 1;
    }

    // `base.<partial>`: a dot directly before the partial word, an identifier
    // directly before the dot (no spaces — member access is tight).
    if word > 0 && prefix[word - 1] == '.' {
        let dot = word - 1;
        let mut start = dot;
        while start > 0 && is_ident_char(prefix[start - 1]) {
            start -= 1;
        }
        let base: String = prefix[start..dot].iter().collect();
        if !base.is_empty() {
            return Context::Member(base);
        }
    }

    // `so <partial>`: the word before the cursor (across spaces) is `so`.
    let mut before = word;
    while before > 0 && prefix[before - 1].is_whitespace() {
        before -= 1;
    }
    let mut prev_start = before;
    while prev_start > 0 && is_ident_char(prefix[prev_start - 1]) {
        prev_start -= 1;
    }
    if prefix[prev_start..before].iter().collect::<String>() == "so" {
        return Context::Import;
    }

    Context::General
}

/// The members of `base`, but only when `base` names a stdlib module the buffer
/// has imported. An unknown base (e.g. an object variable) yields nothing rather
/// than misleading suggestions — object-member completion is future work.
fn member_completions(path: &str, source: &str, base: &str) -> Vec<Completion> {
    if !imported_modules(path, source).contains(base) {
        return Vec::new();
    }
    let Some(module) = stdlib::module(base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for func in module.funcs {
        out.push(Completion {
            label: func.name.to_string(),
            kind: CompletionKind::Member,
        });
    }
    for (name, _) in module.consts {
        out.push(Completion {
            label: name.to_string(),
            kind: CompletionKind::Member,
        });
    }
    out
}

/// Every importable module name.
fn module_name_completions() -> Vec<Completion> {
    MODULES
        .iter()
        .map(|m| Completion {
            label: m.name.to_string(),
            kind: CompletionKind::Module,
        })
        .collect()
}

/// Keywords, builtins, imported modules, and the names in scope at the cursor.
fn general_completions(path: &str, source: &str, line: u32, col: u32) -> Vec<Completion> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |out: &mut Vec<Completion>, label: String, kind: CompletionKind| {
        if seen.insert(label.clone()) {
            out.push(Completion { label, kind });
        }
    };

    for (spelling, kind) in KEYWORDS {
        // `def`/`class` are reserved only to greet Python muscle memory with a
        // hint; they are not Doge keywords, so never offer them.
        if matches!(kind, TokenKind::Def | TokenKind::Class) {
            continue;
        }
        push(&mut out, spelling.to_string(), CompletionKind::Keyword);
    }
    for builtin in BUILTINS {
        push(&mut out, builtin.name.to_string(), CompletionKind::Builtin);
    }
    for module in imported_modules(path, source) {
        push(&mut out, module, CompletionKind::Module);
    }
    for (name, kind) in in_scope_names(path, source, line, col) {
        push(&mut out, name, kind);
    }
    out
}

/// The names visible at (`line`, `col`). Uses the parsed AST when the buffer
/// parses; otherwise falls back to a token scan of declared names.
fn in_scope_names(path: &str, source: &str, line: u32, col: u32) -> Vec<(String, CompletionKind)> {
    match crate::parser::parse(path, source) {
        Ok(script) => {
            let mut callables = HashSet::new();
            collect_callables(&script.stmts, &mut callables);
            let mut names = Vec::new();
            scope_names_at(&script.stmts, line, col, &mut names);
            names
                .into_iter()
                .map(|name| {
                    let kind = if callables.contains(&name) {
                        CompletionKind::Function
                    } else {
                        CompletionKind::Variable
                    };
                    (name, kind)
                })
                .collect()
        }
        Err(_) => lexical_names(path, source)
            .into_iter()
            .map(|name| (name, CompletionKind::Variable))
            .collect(),
    }
}

/// True when the cursor at (`line`, `col`) is at or after `start`.
fn at_or_after(start: Span, line: u32, col: u32) -> bool {
    line > start.line || (line == start.line && col >= start.col)
}

/// True when the cursor at (`line`, `col`) falls in `[start, end)` — `end` is the
/// next sibling's start, or `None` at the end of the block (open upper bound).
fn within(start: Span, end: Option<Span>, line: u32, col: u32) -> bool {
    if !at_or_after(start, line, col) {
        return false;
    }
    match end {
        None => true,
        Some(end) => line < end.line || (line == end.line && col < end.col),
    }
}

/// Collect the names visible at the cursor: this block's hoisted names, plus the
/// scope-introduced names (params, loop vars, error name) of the innermost
/// construct the cursor sits inside. Over-inclusion is preferred to omission —
/// suggesting a slightly out-of-scope name is a smaller harm than hiding a valid
/// one, and the checker still flags a genuine misuse.
fn scope_names_at(stmts: &[Stmt], line: u32, col: u32, out: &mut Vec<String>) {
    for name in hoisted_names(stmts) {
        push_unique(out, name);
    }
    for (i, stmt) in stmts.iter().enumerate() {
        let end = stmts.get(i + 1).map(|next| next.span());
        if !within(stmt.span(), end, line, col) {
            continue;
        }
        match stmt {
            Stmt::FuncDef { params, body, .. } => {
                push_params(params, out);
                scope_names_at(body, line, col, out);
            }
            Stmt::ObjDef { methods, .. } => scope_names_at(methods, line, col, out),
            Stmt::For {
                vars, rest, body, ..
            } => {
                for var in vars {
                    push_unique(out, var.clone());
                }
                if let Some(rest) = rest {
                    push_unique(out, rest.clone());
                }
                scope_names_at(body, line, col, out);
            }
            Stmt::While { body, .. } => scope_names_at(body, line, col, out),
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (_, body) in branches {
                    scope_names_at(body, line, col, out);
                }
                if let Some(body) = else_body {
                    scope_names_at(body, line, col, out);
                }
            }
            Stmt::Try {
                body,
                err_name,
                handler,
                ..
            } => {
                scope_names_at(body, line, col, out);
                push_unique(out, err_name.clone());
                scope_names_at(handler, line, col, out);
            }
            _ => {}
        }
        break;
    }
}

fn push_params(params: &Params, out: &mut Vec<String>) {
    for name in params.binding_names() {
        push_unique(out, name);
    }
}

fn push_unique(out: &mut Vec<String>, name: String) {
    if !out.contains(&name) {
        out.push(name);
    }
}

/// Every function and object (class) name declared anywhere in the program,
/// descending through nested function and object bodies too — used only to tag a
/// completion as a function rather than a plain variable.
fn collect_callables(stmts: &[Stmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::FuncDef { name, body, .. } => {
                out.insert(name.clone());
                collect_callables(body, out);
            }
            Stmt::ObjDef { name, methods, .. } => {
                out.insert(name.clone());
                collect_callables(methods, out);
            }
            other => {
                crate::ast::for_each_child_block(other, &mut |block| collect_callables(block, out))
            }
        }
    }
}

/// The module names the buffer imports (`so nerd`), from the AST when it parses,
/// else from a token scan.
fn imported_modules(path: &str, source: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    match crate::parser::parse(path, source) {
        Ok(script) => collect_imports(&script.stmts, &mut out),
        Err(_) => {
            for name in lexical_imports(path, source) {
                out.insert(name);
            }
        }
    }
    out
}

fn collect_imports(stmts: &[Stmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        if let Stmt::Import { module, .. } = stmt {
            out.insert(module.clone());
        }
        crate::ast::for_each_child_block(stmt, &mut |block| collect_imports(block, out));
    }
}

/// Best-effort declared names from the token stream, for when the buffer does not
/// parse. Recognises the leading token of each binding form (`such`/`so name`,
/// `many Name`, `much` params, `for` vars, `oh no err!`). Flat — no scoping — but
/// enough to keep completion useful mid-edit.
fn lexical_names(path: &str, source: &str) -> Vec<String> {
    let tokens = crate::lexer::lex(path, source).unwrap_or_default();
    let mut out = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        match &token.kind {
            TokenKind::Such | TokenKind::So | TokenKind::Many => {
                if let Some(TokenKind::Ident(name)) = tokens.get(i + 1).map(|t| &t.kind) {
                    push_unique(&mut out, name.clone());
                }
            }
            TokenKind::Much | TokenKind::For => {
                for next in &tokens[i + 1..] {
                    match &next.kind {
                        TokenKind::Ident(name) => push_unique(&mut out, name.clone()),
                        TokenKind::Colon | TokenKind::Newline | TokenKind::In => break,
                        _ => {}
                    }
                }
            }
            TokenKind::OhNo => {
                if let Some(TokenKind::Ident(name)) = tokens.get(i + 1).map(|t| &t.kind) {
                    push_unique(&mut out, name.clone());
                }
            }
            _ => {}
        }
    }
    out
}

fn lexical_imports(path: &str, source: &str) -> Vec<String> {
    let tokens = crate::lexer::lex(path, source).unwrap_or_default();
    let mut out = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if matches!(token.kind, TokenKind::So) {
            if let Some(TokenKind::Ident(name)) = tokens.get(i + 1).map(|t| &t.kind) {
                push_unique(&mut out, name.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(source: &str, line: u32, col: u32) -> Vec<String> {
        complete("buf.doge", source, line, col)
            .into_iter()
            .map(|c| c.label)
            .collect()
    }

    fn completion(source: &str, line: u32, col: u32, label: &str) -> Option<Completion> {
        complete("buf.doge", source, line, col)
            .into_iter()
            .find(|c| c.label == label)
    }

    #[test]
    fn general_offers_keywords_and_builtins() {
        let got = labels("bark \n", 1, 6);
        assert!(got.contains(&"such".to_string()));
        assert!(got.contains(&"if".to_string()));
        assert!(got.contains(&"len".to_string()));
        assert!(got.contains(&"range".to_string()));
    }

    #[test]
    fn reserved_words_are_never_offered() {
        let got = labels("\n", 1, 1);
        assert!(!got.contains(&"def".to_string()));
        assert!(!got.contains(&"class".to_string()));
    }

    #[test]
    fn top_level_names_are_in_scope() {
        // `greet`'s own `wow` closes the function; the trailing `wow` ends the
        // script. The cursor sits on the blank line between them.
        let src = "such greeting = \"hi\"\nsuch greet much name:\n  bark name\nwow\n\nwow\n";
        let got = labels(src, 5, 1);
        assert!(got.contains(&"greeting".to_string()));
        assert!(got.contains(&"greet".to_string()));
        assert_eq!(
            completion(src, 5, 1, "greet").map(|c| c.kind),
            Some(CompletionKind::Function)
        );
        assert_eq!(
            completion(src, 5, 1, "greeting").map(|c| c.kind),
            Some(CompletionKind::Variable)
        );
    }

    #[test]
    fn a_param_is_visible_only_inside_its_function() {
        let src = "such greet much name:\n  bark name\nwow\nwow\n";
        assert!(labels(src, 2, 3).contains(&"name".to_string()));
        let src_after = "such greet much name:\n  bark name\nwow\nsuch other = 1\nwow\n";
        let after = labels(src_after, 4, 1);
        assert!(!after.contains(&"name".to_string()));
        assert!(after.contains(&"other".to_string()));
    }

    #[test]
    fn member_access_offers_module_members() {
        let src = "so nerd\nbark nerd.\n";
        let got = labels(src, 2, 11);
        assert!(got.contains(&"sqrt".to_string()));
        assert!(got.contains(&"floor".to_string()));
        // Not the top-level grab-bag: keywords do not appear after a dot.
        assert!(!got.contains(&"such".to_string()));
    }

    #[test]
    fn member_access_on_unimported_module_is_empty() {
        assert!(labels("bark nerd.\n", 1, 11).is_empty());
    }

    #[test]
    fn import_position_offers_module_names() {
        let got = labels("so \n", 1, 4);
        assert!(got.contains(&"nerd".to_string()));
        assert!(got.contains(&"strings".to_string()));
        assert!(!got.contains(&"len".to_string()));
    }

    #[test]
    fn unparsable_buffer_falls_back_to_declared_names() {
        // A dangling `if` header does not parse, but declared names still surface.
        let src = "such total = 0\nif total >\n";
        let got = labels(src, 2, 11);
        assert!(got.contains(&"total".to_string()));
    }
}
