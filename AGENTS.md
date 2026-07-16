# Doge — agent guide

> A scripting language with Python's ease and Rust's engine: `.doge` scripts transpile to Rust and
> compile to native binaries. Keywords come from doge-speak (`such`, `much`, `wow`, `pls`, `bark`).
> Dynamically typed, indentation-based. `Rc`/`RefCell` runtime — no GC, no `unsafe`.

This is the single source of project instructions. `.claude/CLAUDE.md` imports this file; both Codex
and Claude Code read the same rules from here. Per-crate detail lives in each crate's own `AGENTS.md`
(loaded only when you work in that crate) — keep this root file lean.

## Do this first — task routing

| If the task involves… | First… |
| --- | --- |
| Feature work — "add/implement/build/create/extend/refactor" | Use the **function-index** skill before reading code |
| The language surface — keyword, grammar, operator, type, semantics | Read `docs/SYNTAX.md` + `docs/GRAMMAR.md`; update them in the same change and refresh the **writing-doge** skill if a keyword/stdlib member/CLI behaviour changed |
| Compiler pipeline / crate boundaries | Read `docs/ARCHITECTURE.md`, then the target crate's `AGENTS.md` |
| Error messages / diagnostics | Read `docs/ERRORS.md` — meme framing, precise content |
| Writing a `.doge` program or `examples/*.doge` test | Use the **writing-doge** skill — Python instincts get Doge wrong |
| Adding/removing a crate or folder | Use the **maintaining-agents** skill afterward |
| Where new code belongs | Crate map below — every concern has exactly one home |

## Planner → executor workflow (Sol plans, Luna executes)

This repo is set up for a two-model Codex flow. Codex profiles are user-level settings, so install
the tables in `.codex/user-config.template.toml` into `~/.codex/config.toml`; the handoff artifact
is `.codex/plan.md` (gitignored). See `.codex/README.md` for the full runbook.

- **Sol — planner and orchestrator** (`gpt-5.6-sol`, high reasoning): the default startup model and
  `codex -p planner "<task>"`. Explores, resolves
  design decisions, and writes a precise `.codex/plan.md` (files to touch, exact edits, tests, verify
  commands). **Sol writes only `.codex/plan.md` — no source edits.**
- **Luna — executor** (`gpt-5.6-luna`, high reasoning): `codex -p executor "execute .codex/plan.md"`. Follows
  the plan step by step and runs the verify commands. Luna does not re-explore or re-design — if the
  plan is wrong or incomplete, it stops and hands back to Sol rather than improvising.

The plan is the contract: the frontier model reasons once, the fast model executes against a spec.
A vague plan wastes both. Fill every section of `.codex/PLAN.template.md`.

## Hard rules — never break, not even while debugging

1. **No `unsafe`** — not in runtime, generated code, or "temporarily".
2. **The runtime never panics on user-program errors** — every fallible op returns
   `Result<Value, DogeError>` so `pls`/`oh no` can catch it. Panics are for compiler bugs only.
3. **Language-surface changes update `docs/` in the same change** (`SYNTAX`, `GRAMMAR`, `CLI`) and the
   **writing-doge** skill when a keyword/stdlib member/CLI command changes. Never let spec drift.
4. **One source of truth for keywords** — the single `KEYWORDS` module in `doge-compiler`. Never a
   second keyword list.
5. **Generated Rust is thin glue** — behaviour lives in `doge-runtime`; codegen only wires calls. If
   codegen emits logic, it belongs in the runtime.
6. **Every language feature ships an `examples/*.doge`** that runs as an integration test — untested
   syntax doesn't exist.
7. **Roadmap items beyond the current milestone are not instructions** — implement only when asked.
8. **Doge errors always carry real info** — file, line, caret, concrete fix hint. Never trade clarity
   for the joke (`docs/ERRORS.md`).
9. **Compile-time checks stay honest** — a check is fully enforced or not shipped; no warnings that lie.
10. **Minimal dependencies** — lexer and parser are hand-written. A new crate dep needs a stated reason.
11. **Rust never leaks to the user** — no rustc output, Rust type names, or Rust panics anywhere a Doge
    user sees. Generated code compiles by construction; a rustc rejection or a panic from generated code
    is a Doge compiler bug, reported as one — never shown raw. The sharp-edges guarantees
    (`crates/doge-runtime/AGENTS.md`) are tested, not aspirational.
12. **Never fan out to sub-agents unless explicitly asked** — do the work in the main thread.
13. **Releases, tags, version bumps, and pushes are user-initiated only** — never run `git tag`, push to
    `main`, force-push, bump the workspace version, or run any `gh release` write command on your own
    initiative. Propose the exact commands and stop. See Releases.

## Domain

- **Keywords:** `pls`/`oh no` (try/catch), `bonk` (raise), `bork` (break), `bark` (print), `wow` (closes
  function/object defs and ends the script), `such` (variable with `=`, function with `:` — no `def`),
  `much` (function params, or an object's parent class), `many` (object def; `many Child much Parent:`
  inherits), `super` (parent method), `so` (import / const), plus universal
  `if/elif/else/for/while/in/return/continue`.
- **Pipeline:** lexer (indentation-aware, fuses `oh no`) → recursive-descent parser → AST checks → Rust
  codegen → `rustc`/`cargo` build → cached native binary in `~/.cache/doge/<hash>/`.

Authoritative spec is `docs/` (`README`, `SYNTAX`, `GRAMMAR`, `STDLIB`, `ERRORS`, `ARCHITECTURE`,
`CLI`). When code and docs disagree, that's a bug — fix the mismatch, don't work around it.

## Crate map — one home per concern

Read only the crate relevant to the task; grep before scanning. Each crate has an `AGENTS.md` with its
internal layout.

| Path | Owns |
| --- | --- |
| `crates/doge-runtime/` | What the language *means at runtime* — `Value`, operators, builtins, objects, collection methods, stdlib, concurrency, errors |
| `crates/doge-compiler/` | Turning *source into Rust* — keywords, lexer, parser, AST+walker, checks, diagnostics, builtins table, codegen |
| `crates/doge-interp/` | *Evaluating the checked AST directly* — the tree-walking interpreter behind `doge repl` |
| `crates/doge-lsp/` | The *language server* behind `doge lsp` — thin glue over `doge-compiler` |
| `crates/dogelang/` | The *user's terminal experience* — the `doge` binary, subcommands, build cache |
| `examples/` | `.doge` programs that double as integration tests (`.out` sibling asserts stdout) |
| `docs/` | Authoritative language spec |
| `editors/`, `brand/` | VS Code integration; logo/brand kit |

`check` and `codegen` depend only on `ast`/`builtins`, never each other. A concern never lives in two
crates.

## Conventions

- **Rust:** `cargo fmt`, `cargo clippy -D warnings` clean, no `unsafe`, no `unwrap()`/`expect()` outside
  tests (compiler-internal invariants may use `expect("compiler bug: …")`).
- **Naming:** meme words are user-facing (keywords, CLI, errors); internal Rust uses plain names
  (`parse_try_statement`, not `parse_pls`). AST nodes/tokens mirror the keyword they represent.
- **Tests:** unit tests colocated (`#[cfg(test)]`); every feature also gets an `examples/*.doge`.
- **No magic values, no dead code, no half-finished features on main.**
- **Default to zero comments** — names and structure carry meaning. Comment only a genuine *why* the
  next reader would otherwise get wrong. Never narrate code you just wrote.

## Verify — exact commands (repo root, bash)

1. `cargo fmt --all --check` — fix with `cargo fmt --all`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace` — includes the `examples/*.doge` suite

Run all three, in that order, before any push. Treat any fmt/clippy/test failure as a failing build —
fix it in the same change. `Cargo.lock` is committed; commit lockfile updates alongside dep changes.

## Definition of done

- fmt + clippy + tests all green.
- Language surface touched? `docs/` updated (rule 3) and an `examples/*.doge` added (rule 6);
  writing-doge skill refreshed if a keyword/stdlib member/CLI command changed.
- New diagnostics follow `docs/ERRORS.md` style: meme framing, file/line/caret, fix hint.
- Crate/folder added or removed? Ran the **maintaining-agents** skill.

## Ask, don't assume

- A task implying an unsettled **language-design** decision (new syntax, changed semantics) → stop and
  ask. Never invent language surface silently.
- Genuinely split between two sound designs with real trade-offs → ask, don't pick arbitrarily.
- Scope tightly: a bug fix changes the bug, a feature adds the feature. Flag observed debt; don't
  silently fix it. Turn any manual check into an `examples/*.doge` or unit test in the same change.

## Releases

The only sanctioned release: the **user** (never an agent) pushes an annotated tag `vX.Y.Z` on `main`;
`.github/workflows/release.yml` does the rest (guard → CI verify → draft → build+upload → undraft).
Never `gh release create`/`edit` by hand — it bypasses CI and races the workflow's draft. If a release
run fails: fix on a branch, merge to `main` via PR (`main` only takes merges), delete the leftover
*draft*, delete the old tag, re-tag the new `main` commit. Release notes: fill
`.github/release-notes-template.md` and hand it to the user — never commit filled-in notes.
