# Doge CLI

The `doge` binary and its build cache. Internals of the compile pipeline it drives:
[ARCHITECTURE.md](ARCHITECTURE.md).

## Commands

| Command | Effect |
|---|---|
| `doge bark script.doge [args…]` | compile (cached) and run, forwarding `args…` to the script (`env.args()`); exits with the script's own code |
| `doge build script.doge` | compile (cached) and copy the binary to `./<script-stem>` (`.exe` on Windows) |
| `doge check script.doge` | parse + checks only, no build |
| `doge fmt script.doge` | format the file in place to canonical style; `--check` reports without writing |
| `doge test <file.doge>` \| `doge test <dir>` | discover and run test functions, reporting aggregate pass/fail |
| `doge lsp` | start the language server (LSP over stdin/stdout) for editors — diagnostics and completion |
| `doge repl` (or bare `doge`) | start the interactive interpreter — evaluate Doge without a build |

## Script arguments and input

Anything after the script path in `doge bark script.doge alpha beta` is passed
through to the program and read back with `env.args()` (here `["alpha", "beta"]`);
a standalone `doge build` binary takes its arguments straight from the OS in the
same way. A script reads a line of user input with the `gib` builtin, and touches
files or the environment through the `fetch` and `env` modules
([STDLIB.md](STDLIB.md)). In the REPL, `env.args()` is empty.

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
  available. Importing your own `.doge` modules (by name or string path) is not
  supported in the REPL yet — run the file with `doge bark` to use them.
- **Errors don't end the session.** A syntax, check, or runtime error is reported in
  the usual doge-flavored form ([ERRORS.md](ERRORS.md)) and the prompt returns with
  state intact.
- **Leaving.** Type `wow` on its own line, or press Ctrl-D (EOF).

## Formatting

`doge fmt script.doge` rewrites the file in canonical style; it prints
`such format: <path>` when it changes something and stays silent when the file was
already formatted. `doge fmt --check script.doge` never writes — it exits `0` if the
file is already formatted and non-zero (with a doge-flavored message) if it is not,
for use in CI.

The formatter works on the token stream, not the AST, so it preserves every `#`
comment (own-line and trailing). It only normalizes whitespace — it never adds or
removes line breaks:

- **Indentation** is four spaces per block.
- **Spacing** is normalized: one space around binary operators, `=`, and augmented
  assignments; a space after `,` and after a dict `:`; tight member access (`a.b`),
  call/index parentheses (`f(x)`, `xs[0]`), unary operators (`-1`, `~x`), and slice
  colons (`xs[1:3]`).
- **Blank lines** are capped at one in a row, with leading and trailing blanks
  trimmed and the file ending in a single newline.
- A bracketed expression the author split across lines keeps its line breaks (only
  re-indented); one written on a single line stays on one line.

Formatting first requires the file to **parse** — `doge fmt` on a syntactically
invalid script reports the parser's diagnostic and changes nothing. It never alters
what a script means: the result always lexes to the same token stream, guaranteed by
a check that reports a `very bug. much sorry.` compiler bug rather than emit anything
that would differ.

## Testing

`doge test` runs a Doge test suite and reports aggregate pass/fail in doge-flavored
output. A **test** is a top-level function that takes no arguments and whose name
starts with `test`; its body asserts with `amaze` ([SYNTAX.md](SYNTAX.md) §7). A
`test`-prefixed function that takes arguments is treated as an ordinary helper, not a
test.

```
such test_addition:
    amaze 1 + 1 == 2
wow
```

- `doge test script.doge` runs every test function in that one file.
- `doge test <dir>` discovers every `test_*.doge` file beneath the directory
  (recursively, in path order) and runs each one's tests.

Each test runs on the tree-walking interpreter ([ARCHITECTURE.md](ARCHITECTURE.md)),
so there is no `rustc` build and one test's error never aborts the rest: a failing
`amaze` (or any other catchable runtime error) fails just that test, printed with its
type, message, and `path:line`. `doge test` exits `0` only when at least one test ran
and all of them passed; it exits non-zero if any test fails, any file fails to compile,
or no tests are found at all (`very empty. much untested.`), so CI catches a broken or
empty suite. Tests in one file share that file's top-level state, in the order they are
written.

## Editor integration (language server)

`doge lsp` runs the Doge language server, speaking the Language Server Protocol
over stdin/stdout. Editors spawn it to get:

- **Diagnostics** — the same front end `doge check` runs (`load` + checks), so an
  editor squiggle matches exactly what the CLI reports. The front end stops at the
  first problem, so a file shows at most one diagnostic at a time, cleared as soon
  as it compiles. Errors carry the meme headline and `such fix` hint.
- **Completion** — Doge keywords, the always-in-scope builtins, `so`-imported
  module members (after `nerd.`), module names (after `so `), and the names in
  scope at the cursor (top-level bindings plus the enclosing function's
  parameters and locals). Candidates come from the same tables the compiler uses,
  so completion never drifts from what the language accepts.

The server reads unsaved buffer text, so diagnostics and completion track edits
live. It resolves imported modules from disk; an unsaved sibling module is not yet
seen, and an error in an imported file is surfaced on the active file with the real
`path:line` named in the message. The VS Code extension under `editors/vscode/`
starts the server automatically; the `doge.serverPath` setting points it at the
`doge` binary (default: `doge` on `PATH`).

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
