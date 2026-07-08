# Doge — The Doge Programming Language

> A scripting language with Python's ease of use and Rust's engine underneath: `.doge` scripts transpile to Rust and compile to native binaries. Keywords come from doge-speak (`such`, `much`, `wow`, `pls`, `bark`). Much serious project, very real compiler.

**Project status:** M1 landed — the `doge-runtime` crate exists (`Value`, operators, indexing, `bark`/`len`/`str`/`int`/`float`, unit tests). `doge-compiler` and `doge-cli` are still planned (M2/M3). Update this note as milestones land.

## Start Here — Task Routing

Match the task against this table and do the listed action **before** reading code or writing anything:

| If the task involves… | Then first… |
| --- | --- |
| Feature work — "add", "implement", "build", "create", "extend", "refactor" | Run `/function-index` |
| The language surface — a keyword, grammar rule, operator, type, or semantic change | Read [DESIGN.md](../DESIGN.md) §3–§5 — and update it in the same change |
| Compiler pipeline or crate boundaries (lexer/parser/checks/codegen/runtime) | Read [DESIGN.md](../DESIGN.md) §6 |
| Error messages or diagnostics | Read [DESIGN.md](../DESIGN.md) §7 — meme framing, precise content |
| Adding or removing a crate or folder | Run `/maintaining-claude` afterwards |
| Deciding where any new piece of code belongs | Crate table in Project Structure — every concern has exactly one home |

## Hard Rules

Breaking any of these is never acceptable — including during debugging or spikes.

1. **No `unsafe` anywhere** — not in the runtime, not in generated code, not "temporarily".
2. **The runtime never panics on user-program errors** — every fallible operation returns `Result<Value, DogeError>` so `pls`/`oh no` can catch it. Panics are reserved for compiler bugs.
3. **Language surface changes require a DESIGN.md update in the same change** — keywords, grammar, semantics, and CLI behaviour must never drift from the spec.
4. **One source of truth for keywords** — a single keywords module in `doge-compiler` that the lexer, parser, and diagnostics all use. Never a second keyword list.
5. **Generated Rust is thin glue** — behaviour lives in `doge-runtime`; codegen only wires calls together. If codegen is emitting logic, it belongs in the runtime.
6. **Every language feature ships with a `.doge` example under `examples/`** that runs as an integration test — untested syntax doesn't exist.
7. **Roadmap items (DESIGN.md §8) beyond the current milestone are not instructions** — implement only when explicitly asked.
8. **Doge-flavored errors always carry real information** — file, line, caret, and a concrete fix hint. Never sacrifice clarity for the joke (DESIGN.md §7).
9. **Compile-time checks stay honest** — a check (missing `wow`, const reassignment, undeclared name) is either fully enforced or not shipped; no warnings that lie.
10. **Minimal dependencies** — lexer and parser are hand-written (contextual keywords demand it). A new crate dependency needs a stated reason in the PR/commit.
11. **Rust never leaks to the user** — no rustc output, Rust type names, or Rust panics in anything a Doge user sees. Generated code compiles by construction; a rustc rejection or a panic from generated code is a Doge compiler bug, reported as one (DESIGN.md §2 "sharp edges" table is a tested guarantee, not documentation).
12. **Never fan out to subagents unless explicitly asked** — do the work directly in the main thread. Only spawn agents (Agent tool, workflows, parallel task fan-out) when the user explicitly requests it.

## Domain

Doge is a dynamically typed, indentation-based scripting language:

- **Keywords:** `pls`/`oh no` (try/catch), `bork` (break), `bark` (print), `wow` (closes function/object definitions and ends the script), `such` (variable with `=`, function with `:` — there is no `def`), `much` (parameter introducer in function headers only), `many` (object definition), `so` (import / const), plus universal `if/elif/else/for/while/in/return/continue`.
- **Pipeline:** lexer (indentation-aware, fuses `oh no`) → recursive-descent parser → AST checks → Rust codegen → `rustc`/`cargo` build → cached native binary.
- **Memory model:** `Rc`/`RefCell` reference counting in the runtime — no GC, no `unsafe`.

Full spec — keywords, grammar, semantics, architecture, roadmap: [DESIGN.md](../DESIGN.md). It is the authoritative reference; when code and DESIGN.md disagree, that's a bug in one of them — fix the mismatch, don't work around it.

---

## Stack

| Layer | Technology |
| --- | --- |
| Implementation language | Rust (stable toolchain), Cargo workspace |
| Lexer / parser | Hand-written (indentation tokens, contextual keywords) |
| Runtime values | `Value` enum, `Rc`/`RefCell`, `Result`-based errors |
| Target | Generated Rust source → native binary via `rustc`/`cargo` |
| Build cache | Content hash → `~/.cache/doge/<hash>/` |
| CLI | `doge bark` (run), `doge build`, `doge check` |

---

## Project Structure

```text
crates/
  doge-runtime/     # EXISTS: Value enum, operators/indexing, builtins (bark, len, str/int/float); stdlib (math, strings) comes in M5
  doge-cli/         # PLANNED (M3): `doge` binary — bark/build/check subcommands, build cache, toolchain detection
  doge-compiler/    # PLANNED (M2): lexer, parser, AST, checks, Rust codegen, keywords module (single source of truth)
examples/           # PLANNED (M3): .doge example programs — double as integration tests
DESIGN.md           # authoritative language spec + architecture + roadmap
```

**Where does code belong?** Anything about *what the language means at runtime* → `doge-runtime`. Anything about *turning source into Rust* → `doge-compiler`. Anything about *the user's terminal experience* (subcommands, caching, install hints) → `doge-cli`. A concern never lives in two crates.

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
- **Spec is binding.** If a change pressures the language spec or a crate boundary, update DESIGN.md in the same change — never silently diverge.
- **Explicit over clever.** Obvious code beats a one-liner that needs context — especially in the parser, where the next reader is debugging a syntax error at 2am.
- **Comments only where code can't explain itself** — a grammar ambiguity, a deliberate trade-off, the *why* behind a decision.

---

## Working Approach

**Before writing:**

- Run `/function-index` for every feature request — it identifies the files to read and existing functions to reuse or extend.
- Read only the files you'll touch plus their direct imports. Grep for the existing pattern and match it exactly.
- **Ask before assuming.** If a task implies a language-design decision not settled in DESIGN.md (new syntax, changed semantics, an open question from §9) — stop and ask. Never invent language surface silently.
- **Ask when genuinely split** between two sound designs with real trade-offs. Don't pick arbitrarily.

**While writing:**

- Scope tightly: a bug fix changes the bug, a feature adds the feature. Flag observed debt in your response; don't silently fix it.
- **Turn manual checks into tests.** Ran a snippet by hand to verify behaviour? It becomes an `examples/*.doge` test or a unit test in the same change.
- Run fmt/clippy as you change code; run the full test suite before pushing.

**Definition of done — all must hold:**

- `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test --workspace` all green.
- Language surface touched? DESIGN.md updated in the same change (Hard Rule 3) and an `examples/*.doge` test added (Hard Rule 6).
- New diagnostics follow the DESIGN.md §7 style: meme framing, file/line/caret, fix hint.
- Crate added/removed or folder layout changed? Run `/maintaining-claude`.

**Maintaining these docs:**

- For complex situational context spanning multiple prompts, create `.claude/<topic>.md` + one reference line here; delete it when no longer relevant.
- Never add changelogs or task notes here; git tracks what changed.
