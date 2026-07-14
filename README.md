<p align="center">
  <img src="brand/assets/doge-banner.svg" alt="doge, such language, much wow" width="720">
</p>

<p align="center">
  <a href="https://github.com/DogeLanguage/doge/stargazers"><img src="https://img.shields.io/github/stars/DogeLanguage/doge?style=flat&label=shibes%20wowed&color=f4b400" alt="Stars"></a>
  <a href="https://github.com/DogeLanguage/doge/commits/main"><img src="https://img.shields.io/github/last-commit/DogeLanguage/doge/main?color=f4b400&label=last%20bark" alt="Last commit"></a>
  <a href="https://github.com/DogeLanguage/doge/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/DogeLanguage/doge/ci.yml?branch=main&color=f4b400&label=very%20build" alt="CI"></a>
  <a href="https://github.com/DogeLanguage/doge/issues"><img src="https://img.shields.io/github/issues/DogeLanguage/doge?color=f4b400&label=much%20borks" alt="Open issues"></a>
  <a href="https://github.com/DogeLanguage/doge/blob/main/LICENSE"><img src="https://img.shields.io/github/license/DogeLanguage/doge?color=f4b400&label=such%20license" alt="License"></a>
  <img src="https://img.shields.io/badge/wow-555" alt="wow">
</p>

Doge is a dynamically typed scripting language that reads alot like Python and runs as a native binary. Scripts transpile to Rust and compile ahead of time, so you write clear, low-ceremony code and ship real performance:

- **Native, cached builds** — a `.doge` script becomes Rust source and compiles to a native executable. Builds are content-hashed and cached, so an unchanged script runs instantly.
- **No sharp edges** — one string type with character-based indexing, automatic int/float promotion, and reference-counted values with no GC, no `unsafe`, and no ownership rules to learn.
- **Errors you can catch** — every runtime fault is recoverable with `pls`/`oh no`, and diagnostics point at the exact line with a concrete fix — never a raw Rust error.
- **Doge-speak grammar** — keywords borrow from the meme where it reads well (`such` to declare, `much` for parameters, `bark` to print, `wow` to close a block) and stay universal (`if`, `for`, `while`) where convention wins.

## Example

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

Doge needs a [Rust](https://rustup.rs) toolchain to install and to compile scripts.

```sh
cargo install dogelang
```

The first run pays the Rust compile time; the binary is then cached in `~/.cache/doge/`, so an unchanged script runs instantly.

## Usage

| Command | Effect |
|---|---|
| `doge new <name>` | scaffold a new project (`doge.toml` + `main.doge`) |
| `doge bark script.doge` | compile (cached) and run |
| `doge build script.doge` | compile and copy the binary to `./<name>` |
| `doge check script.doge` | parse and check only, no build |
| `doge repl` (or bare `doge`) | interactive interpreter, evaluate Doge with no build |

The `examples/` folder tours the language; start with `examples/tour.doge`.

## Documentation

| Document | Contents |
|---|---|
| [SYNTAX.md](docs/SYNTAX.md) | Keywords, literals, variables, control flow, functions, error handling, objects, imports |
| [GRAMMAR.md](docs/GRAMMAR.md) | Grammar sketch (EBNF) and disambiguation rules |
| [STDLIB.md](docs/STDLIB.md) | Builtins, list/dict methods, and modules |
| [ERRORS.md](docs/ERRORS.md) | Diagnostic and runtime error message style |
| [CLI.md](docs/CLI.md) | The `doge` binary and the build cache |
| [PACKAGING.md](docs/PACKAGING.md) | Projects, the `doge.toml` manifest, dependencies, install, and sharing |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Compiler pipeline, crate layout, runtime model, codegen |

## License

Apache 2.0, see [LICENSE](LICENSE).
