'use strict';

// Thin glue: launches the Doge language server (`doge lsp`) and connects it to
// VS Code for diagnostics and completion. All language intelligence lives in the
// Rust server; this file only spawns it and wires the transport. The server
// binary is `doge` on PATH by default, overridable with `doge.serverPath`.

const vscode = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function start() {
  const configured = vscode.workspace.getConfiguration('doge').get('serverPath');
  const command = configured && configured.trim() ? configured.trim() : 'doge';

  const invocation = { command, args: ['lsp'], transport: TransportKind.stdio };
  const serverOptions = { run: invocation, debug: invocation };
  const clientOptions = { documentSelector: [{ language: 'doge' }] };

  client = new LanguageClient('doge', 'Doge Language Server', serverOptions, clientOptions);
  client.start();
}

function stop() {
  if (!client) return undefined;
  const stopping = client.stop();
  client = undefined;
  return stopping;
}

module.exports = { start, stop };
