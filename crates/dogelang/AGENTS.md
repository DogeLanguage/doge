# dogelang

**The user's terminal experience.** The `doge` binary (published to crates.io as `dogelang`).
Subcommands, caching, install hints. No language semantics here.

## Layout

- hand-rolled args; `bark`/`build`/`check`/`fmt`/`lsp`/`repl` subcommands. `repl.rs` is the interactive
  loop; `lsp` calls `doge-lsp`; the `DOGE_INTERP` env runs a file through `doge-interp`.
- `cache.rs` — build cache, salted with crate version + codegen rev + a hash of `doge-runtime` source
  (from `build.rs`).
- `src/build.rs` — cargo build glue.

## Rules specific to this crate

- **Rust never leaks** (root rule 11): no rustc output, Rust type names, or Rust panics reach the user.
  A rustc rejection or a panic from generated code is a compiler bug — report it as
  `"very bug. much sorry. pls report: <url>"`, never raw.
- CLI behaviour changes update `docs/CLI.md` and the **writing-doge** skill in the same change (rule 3).
- Cache keys must stay honest: anything that changes generated output must change the salt, or stale
  binaries get served.
