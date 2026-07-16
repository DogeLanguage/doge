# doge-interp

**Evaluating the checked AST directly.** The tree-walking interpreter behind `doge repl`. It calls
`doge-runtime` directly and must match compiled output — the examples parity suite asserts it.

## Layout

- `analyze` — program-wide fn ids + closure captures + the flattened class table.
- `exec` — statements + `Flow`.
- `expr` — operators via a binop map.
- `call` — functions / methods / ctors / `super` / `bind_args`.
- `natives` — builtins + stdlib adapters, driven by the compiler's `BUILTINS`/module tables (not a
  second copy).
- `pack.rs` — `pack.zoom` spawns a fresh interp over the `Arc<Program>` on the pup's thread; the one
  native that needs engine state.

## Rules specific to this crate

- **Parity is the contract:** any behaviour here must match compiled codegen output. Reuse the compiler
  tables and `doge-runtime` value logic — never fork semantics into the interpreter.
- Never panic on a user-program error (root rule 2) — surface a catchable `DogeError`.
