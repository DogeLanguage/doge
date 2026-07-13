# Doge Packaging & Distribution

How Doge programs and their dependencies are packaged, shared, and installed. The
command surface is in [CLI.md](CLI.md); the import semantics are in
[SYNTAX.md](SYNTAX.md) §9.

## Installing the compiler

Doge needs a [Rust](https://rustup.rs) toolchain (it compiles scripts through
`rustc`/`cargo`). Install the `doge` binary from crates.io:

```sh
cargo install dogelang
```

An installed `doge` builds scripts on its own — the generated crate depends on the
matching published `doge-runtime` version, so no source checkout is required.

## Projects and the manifest

A **project** is a directory with a `doge.toml` manifest at its root. Everything a
project needs — its name, entry point, and dependencies — is declared there, and
the project root it sits in is the anchor imports resolve against.

```toml
[package]
name = "my_app"        # required; also the default `doge build` binary name
version = "0.1.0"       # optional, defaults to "0.0.0"
entry = "main.doge"     # optional, defaults to "main.doge"; relative to the root

[dependencies]
greet = { path = "lib/greet" }                                # a local package
cool  = { git = "https://github.com/u/cool", tag = "v1.0.0" } # a git package
```

Scaffold a fresh project with `doge new <name>` — it writes a `doge.toml`, a
runnable `main.doge`, and a `.gitignore`.

`doge bark` and `doge build` run without a script path inside a project: they use
`[package].entry`. Passing a path still works, and a bare script with no `doge.toml`
anywhere above it behaves exactly as before — projects are opt-in.

The manifest format is a small, strict subset of TOML: `[package]` and
`[dependencies]` tables, `key = "string"` pairs, and inline-table dependency values.
Anything outside that subset is a doge-flavored error naming the line.

## Dependencies

A dependency is **itself a project** — a directory with its own `doge.toml`. The
key in `[dependencies]` is the local **alias**: `so <alias>` imports that package's
entry module, exactly like a local module (member access, first-class functions,
constants, and classes all work the same — see [SYNTAX.md](SYNTAX.md) §9). A
dependency may declare its own dependencies; the whole graph is resolved
transitively, and each package sees only the dependencies it declares.

Two sources are supported (there is no central registry):

- **Path** — `{ path = "lib/greet" }` resolves relative to the declaring package's
  root. Best for packages in the same repository or a local checkout.
- **Git** — `{ git = "<url>", tag = "v1.0.0" }` clones a repository. Pin the
  revision with exactly one of `rev` (a commit sha), `tag`, or `branch`; with none,
  the repository's default branch is used.

### Import resolution order

A bare `so <name>` resolves in this order:

1. a built-in stdlib module (`nerd`, `strings`, `fetch`, `env`, `howl`);
2. a **declared dependency** of the package that owns the importing file;
3. a sibling `<name>.doge` next to the importing file.

If a name is *both* a declared dependency and an on-disk sibling, the import is
ambiguous and doge asks you to rename one — it never guesses. A string-path import
`so "rel/path.doge"` always names a local file and is unaffected.

### The lockfile

Fetched git dependencies are recorded in `doge.lock` at the project root, pinning
each `(url, requested-revision)` to the exact commit that was resolved. Commit
`doge.lock` so collaborators and CI build the same commits. Path dependencies need
no lock entry. A git package is cloned once into the shared cache
(`<cache>/deps/git/<url-hash>/<sha>/`) and reused offline on later builds; delete
the lock entry (or the cache) to pick up a newer commit of a moving branch.

## Sharing a program

- **Source** — share the project directory (`doge.toml` + `.doge` files, plus
  `doge.lock`). A recipient with `doge` and a Rust toolchain runs it with
  `doge bark`.
- **Binary** — `doge build` compiles the project to a single native executable
  named after `[package].name`, dropped in the current directory. It is
  self-contained (the runtime is linked in) and needs no doge or Rust toolchain to
  run. `doge build` targets the host platform only; cross-compilation is not yet
  supported.

## Editor support

The language server ([CLI.md](CLI.md)) resolves **path** dependencies from disk, so
a project's cross-package imports get full diagnostics and completion in the editor.
A git dependency that has not been fetched yet surfaces one honest diagnostic
pointing you at `doge bark` (which fetches it) rather than a false "unknown module".
