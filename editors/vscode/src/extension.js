'use strict';

// Thin glue: registers a semantic-token provider that colours each doge-speak
// group with a rainbow colour, and starts the language server client for
// diagnostics and completion. All tokenization logic lives in tokenizer.js (kept
// vscode-free so it is unit-testable) and all language intelligence lives in the
// Rust server (client.js spawns it); this file only wires them to VS Code.

const vscode = require('vscode');
const { tokenize, PALETTE_SIZE } = require('./tokenizer');
const client = require('./client');

const tokenTypes = Array.from({ length: PALETTE_SIZE }, (_, i) => `dogeRainbow${i + 1}`);
const legend = new vscode.SemanticTokensLegend(tokenTypes, []);

const provider = {
  provideDocumentSemanticTokens(document) {
    const builder = new vscode.SemanticTokensBuilder(legend);
    for (const t of tokenize(document.getText())) {
      builder.push(t.line, t.start, t.length, t.colorIndex, 0);
    }
    return builder.build();
  },
};

function activate(context) {
  context.subscriptions.push(
    vscode.languages.registerDocumentSemanticTokensProvider({ language: 'doge' }, provider, legend)
  );
  client.start();
}

function deactivate() {
  return client.stop();
}

module.exports = { activate, deactivate };
