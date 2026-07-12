//! The Doge language server: diagnostics and completion for `.doge` files over
//! LSP. It is thin glue — every language decision lives in `doge-compiler`
//! ([`convert`]); this module speaks the protocol and tracks open buffers.
//!
//! `doge lsp` (in `doge-cli`) calls [`run_stdio`]; the VS Code extension spawns
//! that and talks to it over stdin/stdout.

use std::collections::HashMap;
use std::error::Error;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    PublishDiagnosticsParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    Url,
};

mod convert;
#[cfg(test)]
mod tests;

use convert::{completions_for, diagnostics_for};

/// Errors here are protocol/transport failures reported to the editor's log, not
/// Doge program errors — those are diagnostics, which never surface as an `Err`.
type ServerError = Box<dyn Error + Send + Sync>;

/// Run the language server over stdin/stdout until the client shuts it down.
pub fn run_stdio() -> Result<(), ServerError> {
    let (connection, io_threads) = Connection::stdio();
    let capabilities = serde_json::to_value(server_capabilities())?;
    connection.initialize(capabilities)?;
    main_loop(&connection)?;
    // Drop the connection (and its sending half) before joining, so the writer
    // thread sees the channel close and exits instead of blocking the join.
    drop(connection);
    io_threads.join()?;
    Ok(())
}

/// What this server can do: full-text document sync, and completion triggered on
/// `.` (member access) as well as on request.
fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// The open buffers, keyed by URI. Full-text sync means each change carries the
/// whole document, so the stored text is always the editor's current content.
#[derive(Default)]
struct Documents {
    open: HashMap<Url, String>,
}

impl Documents {
    fn text(&self, uri: &Url) -> Option<&String> {
        self.open.get(uri)
    }
}

fn main_loop(connection: &Connection) -> Result<(), ServerError> {
    let mut docs = Documents::default();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                handle_request(connection, &docs, request)?;
            }
            Message::Notification(notification) => {
                handle_notification(connection, &mut docs, notification)?;
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn handle_request(
    connection: &Connection,
    docs: &Documents,
    request: Request,
) -> Result<(), ServerError> {
    let response = match request.method.as_str() {
        "textDocument/completion" => {
            let params: CompletionParams = serde_json::from_value(request.params)?;
            let items = completions_for(docs, &params);
            Response::new_ok(request.id, CompletionResponse::Array(items))
        }
        other => Response::new_err(
            request.id,
            ErrorCode::MethodNotFound as i32,
            format!("doge-lsp does not handle {other}"),
        ),
    };
    connection.sender.send(Message::Response(response))?;
    Ok(())
}

fn handle_notification(
    connection: &Connection,
    docs: &mut Documents,
    notification: Notification,
) -> Result<(), ServerError> {
    match notification.method.as_str() {
        "textDocument/didOpen" => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(notification.params)?;
            let uri = params.text_document.uri;
            let text = params.text_document.text;
            publish(connection, uri.clone(), &text)?;
            docs.open.insert(uri, text);
        }
        "textDocument/didChange" => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(notification.params)?;
            // Full-text sync: the last change carries the whole document.
            if let Some(change) = params.content_changes.into_iter().last() {
                let uri = params.text_document.uri;
                publish(connection, uri.clone(), &change.text)?;
                docs.open.insert(uri, change.text);
            }
        }
        "textDocument/didSave" => {
            let params: DidSaveTextDocumentParams = serde_json::from_value(notification.params)?;
            if let Some(text) = docs.text(&params.text_document.uri).cloned() {
                publish(connection, params.text_document.uri, &text)?;
            }
        }
        "textDocument/didClose" => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(notification.params)?;
            docs.open.remove(&params.text_document.uri);
            // Clear any squiggles the closed file was showing.
            publish_diagnostics(connection, params.text_document.uri, Vec::new())?;
        }
        _ => {}
    }
    Ok(())
}

fn publish(connection: &Connection, uri: Url, text: &str) -> Result<(), ServerError> {
    let diagnostics = diagnostics_for(&uri, text);
    publish_diagnostics(connection, uri, diagnostics)
}

fn publish_diagnostics(
    connection: &Connection,
    uri: Url,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> Result<(), ServerError> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    let notification = Notification::new("textDocument/publishDiagnostics".to_string(), params);
    connection
        .sender
        .send(Message::Notification(notification))?;
    Ok(())
}
