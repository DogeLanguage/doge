# Doge CLI

The `doge` binary and its build cache. Internals of the compile pipeline it drives:
[ARCHITECTURE.md](ARCHITECTURE.md).

## Commands

| Command | Effect |
|---|---|
| `doge bark script.doge` | compile (cached) and run; exits with the script's own code |
| `doge build script.doge` | compile (cached) and copy the binary to `./<script-stem>` (`.exe` on Windows) |
| `doge check script.doge` | parse + checks only, no build |
| `doge repl` (or bare `doge`) | start the interactive interpreter — evaluate Doge without a build |

## REPL

`doge repl`, or running `doge` with no arguments, starts an interactive session.
Unlike `bark`/`build`, it never invokes `rustc`: each line is parsed, checked, and
evaluated by the tree-walking interpreter ([ARCHITECTURE.md](ARCHITECTURE.md)), so
results appear instantly. The interpreter runs the same `doge-runtime` a compiled
program does, so behaviour is identical — every `examples/*.doge` produces the same
output through both engines (an enforced test).

- **Prompts.** `doge> ` for a new statement, `...   ` while a construct is still
  open. A bare expression is echoed: `1 + 2` prints `3`; a statement like `bark x`
  or `such x = …` prints only what it would in a script.
- **Multi-line input.** A construct that is not finished on one line — a `such …:`
  function or `many …:` object (closed by `wow`), or an `if`/`for`/`while`/`pls`
  block — keeps reading until a **blank line** runs it, Python-style.
- **Session state.** Bindings persist across lines; a later line may use or redefine
  an earlier one (`such` variables, functions, and objects can be redefined, but a
  `so` constant still cannot be reassigned). `so nerd`/`so strings` make the stdlib
  available. Importing your own `.doge` modules is not supported in the REPL yet —
  run the file with `doge bark` to use them.
- **Errors don't end the session.** A syntax, check, or runtime error is reported in
  the usual doge-flavored form ([ERRORS.md](ERRORS.md)) and the prompt returns with
  state intact.
- **Leaving.** Type `wow` on its own line, or press Ctrl-D (EOF).

## Build cache

The key is a hand-rolled FNV-1a 64-bit hash (no hash-crate dependency) over the
compiler version and the source, so a compiler upgrade or a source edit misses the
stale entry. When a script imports other `.doge` files, the key covers every
imported file's path and source too, so editing any module rebuilds. Each script
gets its own tiny Cargo project at
`<cache>/scripts/<hash>/` (Cargo.toml, `src/main.rs`, and `source.doge`) with a path
dependency on `doge-runtime`; all scripts share one `<cache>/target` dir so the
runtime compiles once. A cache hit requires both the built binary and a stored
`source.doge` that reads back byte-identical, which makes hash collisions and torn
writes harmless (mismatch means rebuild). Concurrent `doge` runs on the same script
are serialized by a `build.lock` marker in the script's entry dir, so two builds
never relink one cached binary out from under a run; late arrivals reuse the binary
the lock holder built. The cache lives at `$DOGE_CACHE_DIR`, else
`$XDG_CACHE_HOME/doge`, else `$HOME/.cache/doge`, else `%LOCALAPPDATA%\doge`
(the default Windows location).

## Toolchain handling

Building shells out to `cargo`; its output is captured, never shown. A cargo failure
means the generated Rust didn't compile, which is a Doge bug ("Rust never leaks",
[ERRORS.md](ERRORS.md)). It is reported as `very bug. much sorry.` with the
toolchain output saved to `build.log`, never spilled on screen. If no Rust toolchain
is found, the CLI explains how to install one.
