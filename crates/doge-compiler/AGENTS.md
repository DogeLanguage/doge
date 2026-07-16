# doge-compiler

**Turning source into Rust.** Lexing, parsing, checking, and codegen. Behaviour that a Doge value
*means* belongs in `doge-runtime`, not here.

## Layout

- `keywords` — the single source of truth. The `KEYWORDS` table drives lexer lookup + diagnostic
  spellings. **Never write a second keyword list** (root rule 4).
- `lexer/` — `mod` + `scan` + `strings`. Indentation-aware; fuses `oh no` into one token.
- `parser/` — `mod` + `stmt` + `expr`; `parse_repl` for snippets. Hand-written recursive descent.
- `ast/` — `mod` + `dump` + `analysis`. `Stmt::span`/`Expr::span`, `for_each_child_block` + hoisting +
  `free_names`/`captures` facts, shared by check, codegen, **and** the interpreter.
- `check/` — `mod` + `stmt` + `scopes`; `check_snippet` + `SessionScope` for the REPL.
- `complete.rs` — completion engine (position → candidates) for the LSP, reusing
  `KEYWORDS`/`BUILTINS`/`MODULES` + AST scope facts.
- `diagnostics.rs` — `Diagnostic` + `source_line`/`split_source_lines` helpers.
- `builtins.rs` — the `BuiltinFn` table, single source for check + codegen + interp.
- stdlib module table; `modules/` — `mod` + `diag` module loader.
- `codegen/` — `mod` + `program` + `names` + `analysis` + `callable` + `stmt` + `expr` + `calls` +
  `dispatch`. Rust codegen.

## Rules specific to this crate

- **`check` and `codegen` depend only on `ast`/`builtins`, never each other.**
- **Each big pass is a directory module:** `mod.rs` holds the driver + shared structs, siblings hold
  impl-method groups (`pub(super)`), `tests.rs` holds its tests. New passes follow this shape.
- **Checks stay honest** (root rule 9): fully enforced or not shipped.
- **New diagnostics** follow `docs/ERRORS.md`: meme framing, file/line/caret, concrete fix hint.
- A keyword, grammar rule, or CLI behaviour change updates `docs/` and the **writing-doge** skill in the
  same change (root rule 3).
- Codegen emits thin glue only — if you're writing logic here, it belongs in `doge-runtime` (root rule 5).
