# Doge CLI

The `doge` binary and its build cache. Internals of the compile pipeline it drives:
[ARCHITECTURE.md](ARCHITECTURE.md).

## Commands

| Command | Effect |
|---|---|
| `doge bark script.doge` | compile (cached) and run; exits with the script's own code |
| `doge build script.doge` | compile (cached) and copy the binary to `./<script-stem>` |
| `doge check script.doge` | parse + checks only, no build |

## Build cache

The key is a hand-rolled FNV-1a 64-bit hash (no hash-crate dependency) over the
compiler version and the source, so a compiler upgrade or a source edit misses the
stale entry. Each script gets its own tiny Cargo project at
`<cache>/scripts/<hash>/` (Cargo.toml, `src/main.rs`, and `source.doge`) with a path
dependency on `doge-runtime`; all scripts share one `<cache>/target` dir so the
runtime compiles once. A cache hit requires both the built binary and a stored
`source.doge` that reads back byte-identical, which makes hash collisions and torn
writes harmless (mismatch means rebuild). The cache lives at `$DOGE_CACHE_DIR`, else
`$XDG_CACHE_HOME/doge`, else `$HOME/.cache/doge`.

## Toolchain handling

Building shells out to `cargo`; its output is captured, never shown. A cargo failure
means the generated Rust didn't compile, which is a Doge bug ("Rust never leaks",
[ERRORS.md](ERRORS.md)). It is reported as `very bug. much sorry.` with the
toolchain output saved to `build.log`, never spilled on screen. If no Rust toolchain
is found, the CLI explains how to install one.
