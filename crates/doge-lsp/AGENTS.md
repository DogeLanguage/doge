# doge-lsp

**The language server behind `doge lsp`.** Thin glue over `doge-compiler` — no language logic of its own.

## Layout

- `lib.rs` — protocol loop + the open-buffer map.
- `convert.rs` — `Diagnostic`/`Completion` → `lsp-types`.

## Rules specific to this crate

- **Reuse, don't reimplement:** diagnostics reuse `load` + `check_program` (identical to `doge check`);
  completion reuses `doge_compiler::complete`. If you need new analysis, add it in `doge-compiler` and
  call it here.
- **The only crate with third-party deps** (`lsp-server`, `lsp-types`) — hand-rolling JSON-RPC would be
  more code than a focused sync library. A further dep still needs a stated reason (root rule 10).
