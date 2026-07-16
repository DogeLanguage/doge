# doge-runtime

**What the language means at runtime.** Anything about the behaviour of a Doge value belongs here, not
in codegen. Generated Rust is thin glue over this crate (root rule 5).

## Layout

- `value.rs` / `ordered_map.rs` — the `Value` enum (incl. objects, insertion-ordered dicts, opaque
  `Socket`/`Pup`/`Bowl` handles).
- `ops/` — operators split by concern (arith + compare + index). Matches over `Value` are exhaustive on
  purpose: a new variant is compiler-forced to handle every op.
- builtins — `bark`, `len`, `str`/`int`/`float`, `range`, `iter_value`.
- `objects.rs` — object fields + method dispatch.
- `methods/` — collection methods (`mod` + `list` + `dict`), with shared `expect_int`/`expect_str` arg
  helpers. Reuse these helpers; don't re-parse args by hand.
- stdlib — `nerd`/`strings`/`fetch`/`env`/`howl`/`pack`/`json`/`dson`/`nap`. `serialize.rs` holds the
  shared JSON+DSON emit helpers; `nap.rs` holds clock/sleep/date.
- `pack.rs` — the concurrency boundary: a `Send`-able `Packed` mirror + `pack_value`/`unpack_packed`
  deep-copy + `spawn_pup`, so pups cross threads without sharing `Rc`.
- error model — `bonk` + recursion guard.

## Rules specific to this crate

- **Never panic on a user-program error** (root rule 2). Every fallible op returns
  `Result<Value, DogeError>`. `unwrap()`/`expect()` only in tests.
- New `Value` variant → let the exhaustive matches in `ops/` guide you to every site that must handle it.
- A behaviour needed by both check/codegen and the interpreter still lives here — it's the lowest crate
  both reach.

## Sharp-edges guarantees (tested, not aspirational — root rule 11)

The user must never meet Rust's rough edges. Each row has integration tests:

| Rust pain | Doge instead |
| --- | --- |
| Borrow checker, moves, lifetimes | Don't exist for the user — `Rc`/`RefCell`; assigning/passing never invalidates anything |
| `String` vs `&str`, byte slicing mid-UTF-8 | One `Str`; indexing and `len()` are character-based — `"héllo"[1]` is `"é"` |
| Integer division truncates | `/` always returns Float; `//` is explicit integer division |
| Mixed-type math needs casts | Int, Float, exact Decimal mix with auto-promotion (Int↔Decimal exact; Float↔Decimal is a catchable `TypeError`) |
| Overflow panics/wraps | `Int` is arbitrary precision — never overflows. Spots needing a machine int (index, shift, `range` bound) raise a catchable error, never wrap silently |
| `unwrap()`/`Option`/`Result` ceremony | `none` is an ordinary value; every runtime error is catchable — no unwrap to forget |
| Out-of-bounds indexing kills the program | Catchable runtime error with file/line/caret |
| `.clone()`, `&`, `*`, `let mut` | Invisible — `such x = y` and calls just work |
| Semicolons, expr-vs-stmt rules | Newlines end statements; no semicolons |

If generated code ever panics or `rustc` rejects it, that's a compiler bug reported as
`"very bug. much sorry. pls report: <url>"` plus the internal log — never raw Rust.
