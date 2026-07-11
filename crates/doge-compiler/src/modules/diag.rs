use super::*;

/// The source line a span points at, for building a diagnostic against a file's
/// text (mirrors the `lines` handling in `check`/`codegen`).
fn source_line(source: &str, line: u32) -> String {
    source
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .nth((line as usize).saturating_sub(1))
        .unwrap_or_default()
        .to_string()
}

pub(super) fn diag(path: &str, source: &str, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        path,
        span.line,
        span.col,
        source_line(source, span.line),
        message,
    )
}

/// The "doge has no module named X" diagnostic for the stdlib-only path (used by
/// `single_file_program`), nudging `math` toward `nerd`.
pub(crate) fn unknown_stdlib_module(
    path: &str,
    source: &str,
    name: &str,
    span: Span,
) -> Diagnostic {
    let hint = if name == "math" {
        "much math? such nerd — write so nerd".to_string()
    } else {
        format!("modules: {}", stdlib::module_names())
    };
    diag(
        path,
        source,
        span,
        format!("doge has no module named {name}"),
    )
    .with_headline("very import. much unknown.")
    .with_hint(hint)
}

/// The "no such module, and no file for it either" diagnostic for a user import
/// whose `<name>.doge` is not next to the importing file.
pub(super) fn missing_module_diag(path: &str, source: &str, name: &str, span: Span) -> Diagnostic {
    let hint = if name == "math" {
        "much math? such nerd — write so nerd".to_string()
    } else {
        format!(
            "make {name}.doge next to this file, or import a stdlib module ({})",
            stdlib::module_names()
        )
    };
    diag(
        path,
        source,
        span,
        format!("doge has no module named {name}"),
    )
    .with_headline("very import. much unknown.")
    .with_hint(hint)
}

pub(super) fn read_error_diag(
    path: &str,
    source: &str,
    name: &str,
    target: &Path,
    err: &std::io::Error,
    span: Span,
) -> Diagnostic {
    diag(
        path,
        source,
        span,
        format!("doge found {name}.doge but could not read it: {err}"),
    )
    .with_headline("very import. much unreadable.")
    .with_hint(format!("check the file at {}", target.display()))
}

/// A user file whose name collides with a stdlib module: the import would always
/// mean the stdlib, so the file can never be reached — a name to fix now.
pub(super) fn shadow_diag(path: &str, source: &str, name: &str, span: Span) -> Diagnostic {
    diag(
        path,
        source,
        span,
        format!("{name}.doge shadows the built-in module {name}"),
    )
    .with_headline("very shadow. much confuse.")
    .with_hint(format!("rename your file — {name} is a doge stdlib module"))
}

/// A circular import: `active` is the chain of modules currently being loaded,
/// and `name` closes the loop back onto one of them.
pub(super) fn cycle_diag(
    path: &str,
    source: &str,
    active: &[String],
    name: &str,
    span: Span,
) -> Diagnostic {
    let start = active.iter().position(|m| m == name).unwrap_or(0);
    let mut chain: Vec<&str> = active[start..].iter().map(String::as_str).collect();
    chain.push(name);
    diag(
        path,
        source,
        span,
        format!("import cycle: {}", chain.join(" → ")),
    )
    .with_headline("very loop. much import.")
    .with_hint("break the loop — one of these imports has to go")
}
