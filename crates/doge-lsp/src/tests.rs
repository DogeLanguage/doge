use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::{CompletionParams, DiagnosticSeverity, Url};
use serde_json::json;

use super::*;
use crate::convert::{completions_for, diagnostics_for};

fn uri(path: &str) -> Url {
    Url::parse(path).expect("test uri")
}

#[test]
fn diagnostics_flag_a_bad_program() {
    // `bark` with no expression fails the front end.
    let diagnostics = diagnostics_for(&uri("file:///tmp/bad.doge"), "bark\nwow\n");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(diagnostics[0].source.as_deref(), Some("doge"));
}

#[test]
fn a_valid_program_has_no_diagnostics() {
    let diagnostics = diagnostics_for(&uri("file:///tmp/ok.doge"), "bark \"hi\"\nwow\n");
    assert!(diagnostics.is_empty());
}

#[test]
fn completion_reads_the_open_buffer() {
    let doc = uri("file:///tmp/buf.doge");
    let mut docs = Documents::default();
    docs.open
        .insert(doc.clone(), "so nerd\nbark nerd.\nwow\n".to_string());

    // Cursor just after `nerd.` on line 2 (0-based line 1, character 10).
    let params: CompletionParams = serde_json::from_value(json!({
        "textDocument": { "uri": doc },
        "position": { "line": 1, "character": 10 },
    }))
    .expect("completion params");

    let labels: Vec<String> = completions_for(&docs, &params)
        .into_iter()
        .map(|item| item.label)
        .collect();
    assert!(labels.contains(&"sqrt".to_string()));
    assert!(labels.contains(&"floor".to_string()));
}

#[test]
fn main_loop_answers_completion_and_publishes_diagnostics() {
    let (server, client) = Connection::memory();
    let server_thread = std::thread::spawn(move || main_loop(&server));

    let doc = uri("file:///tmp/live.doge");
    client
        .sender
        .send(Message::Notification(Notification::new(
            "textDocument/didOpen".to_string(),
            json!({ "textDocument": {
                "uri": doc, "languageId": "doge", "version": 1, "text": "bark\nwow\n"
            } }),
        )))
        .unwrap();
    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(1),
            "textDocument/completion".to_string(),
            json!({ "textDocument": { "uri": doc }, "position": { "line": 0, "character": 4 } }),
        )))
        .unwrap();
    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(2),
            "shutdown".to_string(),
            json!(null),
        )))
        .unwrap();
    client
        .sender
        .send(Message::Notification(Notification::new(
            "exit".to_string(),
            json!(null),
        )))
        .unwrap();

    let mut got_diagnostics = false;
    let mut got_completion = false;
    for message in &client.receiver {
        match message {
            Message::Notification(n) if n.method == "textDocument/publishDiagnostics" => {
                got_diagnostics = true;
            }
            Message::Response(r) if r.id == RequestId::from(1) => got_completion = true,
            _ => {}
        }
    }

    server_thread.join().unwrap().unwrap();
    assert!(
        got_diagnostics,
        "server should publish diagnostics on didOpen"
    );
    assert!(
        got_completion,
        "server should answer the completion request"
    );
}
