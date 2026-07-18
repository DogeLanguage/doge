//! Adapt `doge-compiler` diagnostics and completions to LSP types. Language logic
//! stays in the compiler: diagnostics use `load` + `check_program`, and completion
//! uses `doge_compiler::complete`.

use doge_compiler::{Completion, CompletionKind};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, Diagnostic, DiagnosticSeverity, Position,
    Range, Url,
};

use crate::Documents;

/// The diagnostics for `text` (the editor's current buffer for `uri`): zero when
/// it compiles, or the single front-end error otherwise (the compiler stops at
/// the first problem, so a failed check yields exactly one).
pub fn diagnostics_for(uri: &Url, text: &str) -> Vec<Diagnostic> {
    let path = document_path(uri);
    let outcome = match load_document(&path, text) {
        Ok(program) => doge_compiler::check_program(&program),
        Err(diag) => Err(diag),
    };
    match outcome {
        Ok(()) => Vec::new(),
        Err(diag) => vec![to_lsp_diagnostic(&path, &diag)],
    }
}

/// Load `path`'s program, resolving project dependencies when the file lives in a
/// project. The server never fetches: path dependencies resolve from disk, and a
/// git dependency that hasn't been fetched yet surfaces as one honest diagnostic
/// pointing the user at `doge bark` (rather than a false "unknown module").
#[allow(clippy::result_large_err)]
fn load_document(
    path: &str,
    text: &str,
) -> Result<doge_compiler::Program, doge_compiler::Diagnostic> {
    let start = std::path::Path::new(path)
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    match doge_compiler::discover_root(&start) {
        Some(root) => {
            let mut git = |_url: &str, _rev: &doge_compiler::GitRev| {
                Err("this git dependency isn't fetched yet".to_string())
            };
            let deps = doge_compiler::resolve_project(&root, &mut git)?;
            doge_compiler::load_program_with_deps(path, text, deps)
        }
        None => doge_compiler::load(path, text),
    }
}

/// The completion items offered at the request's cursor position, from the
/// buffer the editor last sent us. An unknown document yields nothing.
pub fn completions_for(docs: &Documents, params: &CompletionParams) -> Vec<CompletionItem> {
    let uri = &params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let Some(text) = docs.text(uri) else {
        return Vec::new();
    };
    let path = document_path(uri);
    // LSP positions are 0-based; the compiler's spans are 1-based.
    doge_compiler::complete(&path, text, position.line + 1, position.character + 1)
        .into_iter()
        .map(to_completion_item)
        .collect()
}

/// A filesystem path for `uri` (so imports resolve from disk), falling back to
/// the raw URI string for a non-file document.
fn document_path(uri: &Url) -> String {
    uri.to_file_path()
        .ok()
        .and_then(|path| path.to_str().map(str::to_string))
        .unwrap_or_else(|| uri.to_string())
}

/// Map one doge [`doge_compiler::Diagnostic`] to an LSP diagnostic, keeping the
/// meme headline and `such fix` hint so the doge flavor reaches the editor. When
/// the error is in an imported file rather than the active one, it is anchored at
/// the top and its real location is named in the message (v1 does not open other
/// files to place the squiggle).
fn to_lsp_diagnostic(doc_path: &str, diag: &doge_compiler::Diagnostic) -> Diagnostic {
    let same_file = diag.path == doc_path;
    let position = if same_file {
        Position::new(diag.line.saturating_sub(1), diag.col.saturating_sub(1))
    } else {
        Position::new(0, 0)
    };
    let end = Position::new(position.line, position.character + 1);

    let mut message = format!("{}\n\n{}", diag.headline, diag.message);
    if !same_file {
        message.push_str(&format!("\n\nin {}:{}", diag.path, diag.line));
    }
    if let Some(hint) = &diag.hint {
        message.push_str(&format!("\n\nsuch fix: {hint}"));
    }

    Diagnostic {
        range: Range::new(position, end),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("doge".to_string()),
        message,
        ..Default::default()
    }
}

fn to_completion_item(completion: Completion) -> CompletionItem {
    CompletionItem {
        label: completion.label,
        kind: Some(to_completion_kind(completion.kind)),
        ..Default::default()
    }
}

fn to_completion_kind(kind: CompletionKind) -> CompletionItemKind {
    match kind {
        CompletionKind::Keyword => CompletionItemKind::KEYWORD,
        CompletionKind::Builtin | CompletionKind::Function => CompletionItemKind::FUNCTION,
        CompletionKind::Variable => CompletionItemKind::VARIABLE,
        CompletionKind::Module => CompletionItemKind::MODULE,
        CompletionKind::Member => CompletionItemKind::METHOD,
    }
}
