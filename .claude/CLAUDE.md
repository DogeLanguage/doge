# Doge — The Doge Programming Language

> A scripting language with Python's ease of use and Rust's engine underneath: `.doge` scripts transpile to Rust and compile to native binaries. Keywords come from doge-speak (`such`, `much`, `wow`, `pls`, `bark`). Much serious project, very real compiler.


## Start Here — Task Routing

Match the task against this table and do the listed action **before** reading code or writing anything:

| If the task involves… | Then first… |
| --- | --- |
| Feature work — "add", "implement", "build", "create", "extend", "refactor" | Run `/function-index` |
| The language surface — a keyword, grammar rule, operator, type, or semantic change | Read [docs/SYNTAX.md](../docs/SYNTAX.md) and [docs/GRAMMAR.md](../docs/GRAMMAR.md) — and update them in the same change |
| Compiler pipeline or crate boundaries (lexer/parser/checks/codegen/runtime) | Read [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) |
| Error messages or diagnostics | Read [docs/ERRORS.md](../docs/ERRORS.md) — meme framing, precise content |
| Adding or removing a crate or folder | Run `/maintaining-claude` afterwards |
| Deciding where any new piece of code belongs | Crate table in Project Structure — every concern has exactly one home |

## Hard Rules

Breaking any of these is never acceptable — including during debugging or spikes.

1. **No `unsafe` anywhere** — not in the runtime, not in generated code, not "temporarily".
2. **The runtime never panics on user-program errors** — every fallible operation returns `Result<Value, DogeError>` so `pls`/`oh no` can catch it. Panics are reserved for compiler bugs.
3. **Language surface changes require a docs/ update in the same change** — keywords, grammar, semantics, and CLI behaviour must never drift from the spec ([docs/SYNTAX.md](../docs/SYNTAX.md), [docs/GRAMMAR.md](../docs/GRAMMAR.md), [docs/CLI.md](../docs/CLI.md)).
4. **One source of truth for keywords** — a single keywords module in `doge-compiler` that the lexer, parser, and diagnostics all use. Never a second keyword list.
5. **Generated Rust is thin glue** — behaviour lives in `doge-runtime`; codegen only wires calls together. If codegen is emitting logic, it belongs in the runtime.
6. **Every language feature ships with a `.doge` example under `examples/`** that runs as an integration test — untested syntax doesn't exist.
7. **Roadmap items (tracked as GitHub issues) beyond the current milestone are not instructions** — implement only when explicitly asked.
8. **Doge-flavored errors always carry real information** — file, line, caret, and a concrete fix hint. Never sacrifice clarity for the joke ([docs/ERRORS.md](../docs/ERRORS.md)).
9. **Compile-time checks stay honest** — a check (missing `wow`, const reassignment, undeclared name) is either fully enforced or not shipped; no warnings that lie.
10. **Minimal dependencies** — lexer and parser are hand-written (contextual keywords demand it). A new crate dependency needs a stated reason in the PR/commit.
11. **Rust never leaks to the user** — no rustc output, Rust type names, or Rust panics in anything a Doge user sees. Generated code compiles by construction; a rustc rejection or a panic from generated code is a Doge compiler bug, reported as one (the "sharp edges" table in the Design rationale section below is a tested guarantee, not documentation).
12. **Never fan out to subagents unless explicitly asked** — do the work directly in the main thread. Only spawn agents (Agent tool, workflows, parallel task fan-out) when the user explicitly requests it.
13. **Releases, tags, version bumps, and pushes are user-initiated only** — never run `git tag`, push tags, push to `main`, force-push, bump the workspace version, or run any `gh release` write command (`create`/`edit`/`upload`/`delete`) on your own initiative. Propose the exact commands and stop; the user runs them. See the Releases section for the only sanctioned release path.

## Releases

The only sanctioned way to release: the user (never Claude) pushes an annotated tag `vX.Y.Z` on `main`. `.github/workflows/release.yml` does everything else — guard (tag must equal the `[workspace.package]` version in `Cargo.toml`, tagged commit must be on `main`, no published release may already exist for the tag) → full CI verify → draft release → 3-target build + asset upload → undraft.

Never create or publish a release by hand with `gh release create`/`edit`. A hand-made release bypasses CI verification, races the workflow's own draft on the same tag, and receives whatever partial assets the build jobs happen to upload. If a release run fails: fix the cause on a branch and merge it to `main` via PR like any other change (`main` only takes merges — never a direct push or hotfix commit), delete the leftover *draft*, then delete the old tag and re-tag the new `main` commit — never undraft or patch a release manually.

## Domain

Doge is a dynamically typed, indentation-based scripting language:

- **Keywords:** `pls`/`oh no` (try/catch), `bonk` (raise), `bork` (break), `bark` (print), `wow` (closes function/object definitions and ends the script), `such` (variable with `=`, function with `:` — there is no `def`), `much` (introduces a function header's parameters, or an object header's parent class), `many` (object definition, `many Child much Parent:` inherits), `super` (call a parent method from inside a method), `so` (import / const), plus universal `if/elif/else/for/while/in/return/continue`.
- **Pipeline:** lexer (indentation-aware, fuses `oh no`) → recursive-descent parser → AST checks → Rust codegen → `rustc`/`cargo` build → cached native binary.
- **Memory model:** `Rc`/`RefCell` reference counting in the runtime — no GC, no `unsafe`.

Full spec — keywords, grammar, semantics, architecture, roadmap: [docs/](../docs/README.md). It is the authoritative reference; when code and the docs disagree, that's a bug in one of them — fix the mismatch, don't work around it.

---

## Design rationale

### Trade-off accepted with transpilation

First run of a script pays rustc compile time (seconds). Mitigations:

- Aggressive build caching: hash the source, keep compiled binaries in
  `~/.cache/doge/`, so `doge bark script.doge` is instant when unchanged.
- Generated Rust is thin glue over a precompiled `doge-runtime` crate, so per-script
  compile units stay small.
- A REPL/interpreter mode is deferred to a later milestone (transpilation doesn't suit
  interactive use).

### Shielding users from Rust's sharp edges

Doge compiles to Rust, but Rust's weird and illogical-feeling parts must never reach
the user. These are guarantees, not aspirations; each one gets integration tests:

| Rust pain | What Doge does instead |
|---|---|
| Borrow checker, ownership, moves, lifetimes | Don't exist for the user. The runtime uses `Rc`/`RefCell`; assigning or passing a value never "moves it away" or invalidates anything |
| `String` vs `&str`, byte-indexed slicing that can panic mid-UTF-8 | One `Str` type. Indexing and `len()` are character-based, never byte-based: `"héllo"[1]` is `"é"` |
| Integer division truncates (`5 / 2 == 2`) | `/` always returns a Float; `//` is explicit integer division |
| Mixed-type math needs casts (`x as f64`) | Int and Float mix freely; promotion is automatic |
| Overflow panics in debug builds but silently wraps in release | Overflow is a catchable runtime error (`pls`/`oh no`) with the same behaviour in every build, never silent wraparound |
| `unwrap()` panics; `Option`/`Result` ceremony everywhere | `none` is an ordinary value; every runtime error is catchable with `pls`/`oh no`, so there is no unwrap to forget |
| Out-of-bounds indexing panics and kills the program | Catchable runtime error with file/line/caret |
| `.clone()`, `&`, `*`, `let mut` ceremony | Invisible: `such x = y` and function calls just work |
| Multi-screen compiler errors about trait bounds | Doge diagnostics: one issue at a time, file/line/caret, concrete fix hint ([docs/ERRORS.md](../docs/ERRORS.md)) |
| Semicolons and expression-vs-statement rules | Newlines end statements; there are no semicolons |

The "Rust never leaks" rule: the user must never see rustc output, Rust type
names, or a Rust panic. Generated code always compiles by construction. If `rustc`
rejects it, or generated code panics at runtime, that is a Doge compiler bug and is
reported as one (`"very bug. much sorry. pls report: <url>"` plus the internal log),
never shown as raw Rust errors.

---

## Stack

| Layer | Technology |
| --- | --- |
| Implementation language | Rust (stable toolchain), Cargo workspace |
| Lexer / parser | Hand-written (indentation tokens, contextual keywords) |
| Runtime values | `Value` enum, `Rc`/`RefCell`, `Result`-based errors |
| Target | Generated Rust source → native binary via `rustc`/`cargo` |
| Build cache | Content hash → `~/.cache/doge/<hash>/` |
| CLI | `doge bark` (run), `doge build`, `doge check`, `doge repl` (interpreter), `doge lsp` (language server) |

---

## Project Structure

```text
crates/
  doge-runtime/     # EXISTS: Value enum (incl. objects, insertion-ordered dicts via ordered_map.rs, opaque Socket/Pup/Bowl handles), operators split by concern (ops/ — arith+compare+index; exhaustive Value matches so a new variant is compiler-forced), builtins (bark, len, str/int/float, range, iter_value), object fields/dispatch (objects.rs), collection methods (methods/ — mod+list+dict, shared expect_int/expect_str arg helpers), stdlib (nerd/strings/fetch/env/howl/pack), concurrency boundary (pack.rs — Send-able Packed mirror + pack_value/unpack_packed deep-copy + spawn_pup, so pups cross threads without sharing Rc), error model (bonk/recursion guard)
  doge-compiler/    # EXISTS: keywords (single source of truth — KEYWORDS table drives lexer lookup + diagnostic spellings), lexer/ (mod+scan+strings), parser/ (mod+stmt+expr; parse_repl for snippets), AST + shared walker (ast/ — mod+dump+analysis; Stmt::span/Expr::span, for_each_child_block + hoisting + free_names/captures facts used by both check, codegen, and the interpreter), semantic checks (check/ — mod+stmt+scopes; check_snippet + SessionScope for the REPL), completion engine (complete.rs — position→candidates for the language server, reusing KEYWORDS/BUILTINS/MODULES + AST scope facts), diagnostics (diagnostics.rs — Diagnostic + source_line/split_source_lines helpers), builtins table (builtins.rs — BuiltinFn, single source for check + codegen + interp), stdlib module table, module loader (modules/ — mod+diag), Rust codegen (codegen/ — mod+program+names+analysis+callable+stmt+expr+calls+dispatch). check and codegen depend only on ast/builtins, never each other. Each big pass is a directory module: mod.rs holds the driver + shared structs, siblings hold impl-method groups (pub(super)), tests.rs holds its tests
  doge-interp/      # EXISTS: tree-walking interpreter over the checked AST (powers `doge repl`) — analyze (program-wide fn ids + closure captures + flattened class table), exec (statements+Flow), expr (operators via binop map), call (functions/methods/ctors/super/bind_args), natives (builtins+stdlib adapters driven by the compiler tables), pack.rs (pack.zoom spawns a fresh interp over the Arc<Program> on the pup's thread — the one native needing engine state). Calls doge-runtime directly; the examples parity suite asserts it matches compiled output
  doge-lsp/         # EXISTS: language server (powers `doge lsp`) — thin LSP glue over doge-compiler: diagnostics reuse load+check_program (identical to `doge check`), completion reuses doge_compiler::complete. lib.rs (protocol loop + open-buffer map), convert.rs (Diagnostic/Completion → lsp-types). The ONLY crate with third-party deps (lsp-server, lsp-types) — hand-rolling JSON-RPC would be more code than a focused sync library
  dogelang/         # EXISTS: `doge` binary (published to crates.io as `dogelang`) — hand-rolled args, bark/build/check/fmt/lsp/repl subcommands (repl.rs is the interactive loop; lsp calls doge-lsp; DOGE_INTERP env runs a file through doge-interp), build cache (cache.rs, salted with crate version + codegen rev + a hash of doge-runtime source from build.rs) + cargo build glue (src/build.rs)
examples/           # EXISTS: .doge example programs (hello, tour, objects, control_flow, collections) — double as integration tests; a `.out` sibling means the example runs and its stdout is asserted
docs/               # EXISTS: authoritative language spec — SYNTAX, GRAMMAR, STDLIB, ERRORS, ARCHITECTURE, CLI
brand/              # EXISTS: logo/brand kit — mark, lockup, banner, favicon SVGs + icon exporter and brand guide
editors/            # EXISTS: editor integrations — vscode/ (.doge language association + file icon + rainbow syntax highlighting: TextMate grammar in syntaxes/ + per-group semantic-token provider in src/, tokenizer unit-tested via `node --test`; plus a language client in src/client.js that spawns `doge lsp` for diagnostics + completion, configured by the doge.serverPath setting)
```

**Where does code belong?** Anything about *what the language means at runtime* → `doge-runtime`. Anything about *turning source into Rust* → `doge-compiler`. Anything about *evaluating the checked AST directly* (the REPL/interpreter engine) → `doge-interp`. Anything about *the user's terminal experience* (subcommands, caching, install hints) → `dogelang`. A concern never lives in two crates.

**Navigation rule:** read only the crate relevant to the task. Grep before scanning.

---

## Conventions

- **Rust:** `cargo fmt` formatting, `cargo clippy` clean with `-D warnings`, no `unsafe`, no `unwrap()`/`expect()` outside tests (compiler-internal invariants may use `expect("compiler bug: …")`).
- **Naming:** meme words are user-facing (keywords, CLI, error framing); internal Rust code uses plain descriptive names (`parse_try_statement`, not `parse_pls`). Exception: AST nodes and tokens mirror the keyword they represent.
- **Tests:** unit tests colocated (`#[cfg(test)]`) per module; every language feature gets an `examples/*.doge` integration test asserting output (and error tests asserting the diagnostic).
- **General:** no dead code or commented-out blocks; no half-finished features on main.

---

## Testing & Lint — exact commands

Run from the repo root (Linux/bash):

1. **Format check** (fastest feedback): `cargo fmt --all --check` — fix with `cargo fmt --all`.
2. **Lint:** `cargo clippy --workspace --all-targets -- -D warnings`
3. **Tests:** `cargo test --workspace` — includes the `examples/*.doge` integration suite.
4. **Before every push:** all three, in that order.

Treat any fmt/clippy/test failure as a failing build — fix it in the same change, never leave it for a follow-up. `Cargo.lock` is committed; after changing any dependency, commit the updated lock file in the same change.

---

## Maintainability

Write for the developer maintaining this 12 months from now.

- **No magic values.** Keyword strings → the keywords module; cache paths, CLI strings, limits → a constants module per crate.
- **Consistent patterns.** New AST nodes, checks, and codegen arms follow the existing shape — read one existing example first.
- **Single source per behaviour.** Before writing the same block a second time, lift it into a shared helper in the lowest crate that both users can reach.
- **Spec is binding.** If a change pressures the language spec or a crate boundary, update docs/ in the same change — never silently diverge.
- **Explicit over clever.** Obvious code beats a one-liner that needs context — especially in the parser, where the next reader is debugging a syntax error at 2am.
- **Default to zero comments.** Do not narrate what the code does — names and structure must carry that. Write a comment *only* when the code genuinely cannot explain itself: a grammar ambiguity, a non-obvious trade-off, or a *why* the next reader would otherwise get wrong. If in doubt, leave it out. Never add comments to explain code you just wrote.

---

## Working Approach

**Before writing:**

- Run `/function-index` for every feature request — it identifies the files to read and existing functions to reuse or extend.
- Read only the files you'll touch plus their direct imports. Grep for the existing pattern and match it exactly.
- **Ask before assuming.** If a task implies a language-design decision not settled in docs/ (new syntax, changed semantics, anything beyond what the spec already settles) — stop and ask. Never invent language surface silently.
- **Ask when genuinely split** between two sound designs with real trade-offs. Don't pick arbitrarily.

**While writing:**

- Scope tightly: a bug fix changes the bug, a feature adds the feature. Flag observed debt in your response; don't silently fix it.
- **Turn manual checks into tests.** Ran a snippet by hand to verify behaviour? It becomes an `examples/*.doge` test or a unit test in the same change.
- Run fmt/clippy as you change code; run the full test suite before pushing.

**Definition of done — all must hold:**

- `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test --workspace` all green.
- Language surface touched? docs/ updated in the same change (Hard Rule 3) and an `examples/*.doge` test added (Hard Rule 6).
- New diagnostics follow the docs/ERRORS.md style: meme framing, file/line/caret, fix hint.
- Crate added/removed or folder layout changed? Run `/maintaining-claude`.

**Maintaining these docs:**

- For complex situational context spanning multiple prompts, create `.claude/<topic>.md` + one reference line here; delete it when no longer relevant.
- Never add changelogs or task notes here; git tracks what changed.
