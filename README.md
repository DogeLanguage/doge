<p align="center">
  <img src="brand/assets/doge-banner.svg" alt="doge, such language, much wow" width="720">
</p>

Doge is a scripting language with the ease of Python and the engine of Rust underneath. Rust's skill floor — ownership, lifetimes, the borrow checker — is too high for casual scripting, while Python proved that a clear, low-ceremony language is what most people reach for. Doge aims to be that language:

- **Rust underneath** — reference counting, no GC, no `unsafe`.
- **Transpiled to native** — a `.doge` script becomes Rust source, built by `rustc` into a native binary. Native speed, never a line of Rust.
- **Wrapped in the meme** — keywords come from doge-speak where it aids readability, staying universal (`if`, `for`, `while`) where convention wins.

```doge
so nerd

such greet much name:
    bark "much hello " + name
wow

for shibe in ["kabosu", "cheems", "walter"]:
    pls
        greet(shibe)
    oh no err!
        bark "very error: " + err

bark "sqrt of 16 is " + str(nerd.sqrt(16))
wow
```

## Installation

Doge needs a Rust toolchain to install and to compile scripts — get one from
[rustup.rs](https://rustup.rs) if you don't have it.

```sh
git clone https://github.com/DogeLanguage/doge
cd doge
cargo install --path crates/doge-cli
doge bark examples/hello.doge   # check that it worked
```

The first run pays the Rust compile time (a few seconds); the binary is then
cached in `~/.cache/doge/`, so an unchanged script runs instantly.

## Usage

| Command | Effect |
|---|---|
| `doge bark script.doge` | compile (cached) and run |
| `doge build script.doge` | compile and copy the binary to `./<script-stem>` |
| `doge check script.doge` | parse and check only, no build |

The `examples/` folder tours the language; start with `examples/tour.doge`.

## Documentation

| Document | Contents |
|---|---|
| [SYNTAX.md](docs/SYNTAX.md) | Keywords, literals, variables, control flow, functions, error handling, objects, imports |
| [GRAMMAR.md](docs/GRAMMAR.md) | Grammar sketch (EBNF) and disambiguation rules |
| [STDLIB.md](docs/STDLIB.md) | Builtins and the `nerd`, `strings`, `lists` modules |
| [ERRORS.md](docs/ERRORS.md) | Diagnostic and runtime error message style |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Compiler pipeline, crate layout, runtime model, codegen |
| [CLI.md](docs/CLI.md) | The `doge` binary and the build cache |

## Status

The core language works end-to-end: variables, control flow, functions,
`pls`/`oh no` error handling, `many Name:` objects, and the `nerd`/`strings`/`lists`
stdlib. Remaining features (closures, first-class functions, `.doge` imports, a REPL)
are tracked as [issues](https://github.com/DogeLanguage/doge/issues).

## License

Apache 2.0, see [LICENSE](LICENSE).
