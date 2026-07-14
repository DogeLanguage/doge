# Doge Architecture

How a `.doge` script becomes a native binary. The user-facing command surface is in
[CLI.md](CLI.md).

## 1. Pipeline

```
script.doge
   │  lexer        (indentation-aware; fuses "oh no"; INDENT/DEDENT tokens)
   ▼
tokens
   │  parser       (hand-written recursive descent; contextual keywords)
   ▼
AST
   │  loader       (follow `so` imports; load user `.doge` modules; cycle detection)
   ▼
Program (entry + modules)
   │  checks       (missing `wow`, assignment to const, undeclared names, bork outside loop;
   │                plus "a module only defines things" for imported modules)
   ▼
   │  codegen      (emit one Rust source wiring every file together, using doge-runtime)
   ▼
generated .rs  ──rustc/cargo──►  native binary  ──►  cached & executed
```

A `so <name>` import resolves in order to a built-in module
(`nerd`/`strings`/`fetch`/`env`/`howl`/`pack`/`json`/`dson`), a declared **dependency** of the importing file's
project, or the user file `<name>.doge` next to the importer. When the entry lives
in a project (a directory with a `doge.toml`), the CLI first resolves the manifest's
dependency graph into a map of package-root → alias → entry file and hands it to the
loader; a bare script with no manifest resolves against the stdlib and siblings only,
exactly as before. The loader parses every reachable file into one `Program`; the
whole downstream pipeline works on that. Codegen keeps every file's top-level names in one flat Rust namespace by
mangling with a per-file id: the entry (file 0) keeps its plain `f_`/`v_` scheme,
so single-file output is unchanged, and a module (file N) carries its id right
after the prefix (`f1_square`, `g1_ANSWER`) — a digit can't start a doge
identifier, so these never collide with the entry's names. A multi-file program
also embeds a per-file source table so an uncaught runtime error reports the
module and line it actually came from.

## 1a. Interpreter path (`doge-interp`)

The transpile-and-build pipeline is the way a `.doge` file becomes a binary. For
`doge repl` — and any evaluation that should skip the rustc build — a second engine,
`doge-interp`, walks the same checked AST and evaluates it directly:

```
tokens → AST → (Program) → doge-interp: tree-walk → doge-runtime
```

It reuses the whole front end (`parse_repl` for snippets, `check_snippet` for a
REPL session's accumulated scope) and calls the exact `doge-runtime` functions
codegen would emit — the same operators, builtins, `builtin_method`, error model,
and `Rc`/`RefCell` cells. It mirrors codegen's static facts (program-wide function
ids with per-closure capture names, and a flattened class table with `super`
resolved up the ancestry) so closures, objects, and inheritance behave identically.
Because behaviour lives entirely in `doge-runtime`, the two engines cannot diverge:
every `examples/*.doge` with a `.out` is run through the interpreter as well and must
produce the same bytes. The interpreter recurses on the native stack (one Doge call
nests several Rust frames), so the CLI runs it on a large-stack thread; the catchable
recursion limit, not a stack overflow, is what stops runaway recursion.

## 1b. Language server path (`doge-lsp`)

`doge lsp` serves editors over LSP (stdin/stdout). Like `doge-interp`, it is a
consumer of the front end, not a new copy of it:

```
editor ⇄ doge-lsp ⇄ doge-compiler (load + checks; complete)
```

Diagnostics run the exact `load` + `check_program` path `doge check` uses, so an
editor squiggle and the CLI never disagree. Completion calls
`doge_compiler::complete`, which reads the single-source keyword/builtin/module
tables and computes in-scope names from the parsed AST — falling back to a token
scan of declared names when a mid-edit buffer does not parse. `doge-lsp` itself
holds no language knowledge: it tracks open buffers and maps `doge-compiler`
results to `lsp-types`. It is the only crate with third-party dependencies
(`lsp-server`, `lsp-types`), since hand-rolling the JSON-RPC protocol would be far
more code than a focused, synchronous LSP library.

## 1c. Dependency resolution boundary

Projects and dependencies ([PACKAGING.md](PACKAGING.md)) split cleanly across the
crate boundary. `doge-compiler` owns everything that is pure parsing and
filesystem: parsing `doge.toml` (`manifest`), and walking the dependency graph into
a package-root → alias → entry-file map (`project::resolve_project`), resolving
`path` dependencies from disk. It never touches the network or the cache — git
sources are handed to a caller-supplied closure. `dogelang` supplies that closure:
it shells out to `git`, caches clones under `<cache>/deps`, and pins resolved commits
in `doge.lock`. The language server (`doge-lsp`) supplies a closure that only reads
an already-fetched clone, so editors resolve path dependencies without ever running
git. This keeps `doge-compiler` free of third-party and I/O concerns while both the
CLI and the LSP reuse one resolver.

## 2. Crate layout

```
doge/
├── Cargo.toml            # workspace ([workspace.package] shares version/edition)
├── rust-toolchain.toml   # pinned stable toolchain (rustfmt + clippy)
├── crates/
│   ├── dogelang/         # `doge` binary: main + build + cache + new (scaffold) +
│   │                     #   deps (git fetch + doge.lock); build.rs salts the
│   │                     #   cache key with a hash of the doge-runtime source
│   ├── doge-compiler/    # each pass is a directory module:
│   │                     #   lexer/ (mod, scan, strings)
│   │                     #   parser/ (mod, stmt, expr)
│   │                     #   check/ (mod, stmt, scopes)
│   │                     #   codegen/ (mod, program, names, analysis, callable,
│   │                     #             stmt, expr, calls, dispatch)
│   │                     #   modules/ (mod, diag)  — the import loader
│   │                     #   ast/ (mod, dump)      — nodes + shared AST walker
│   │                     #   manifest, project     — doge.toml + dependency graph
│   │                     #   plus keywords, token, builtins, stdlib, diagnostics
│   ├── doge-runtime/     # Value enum, ops/ (arith, compare, index), methods/
│   │                     #   (list, dict), builtins, objects, pack (Send boundary),
│   │                     #   stdlib/ (nerd, strings, fetch, env, howl, pack,
│   │                     #   json, dson)
│   ├── doge-interp/      # tree-walking interpreter over the checked AST (doge repl):
│   │                     #   analyze (fn ids + captures + class table), exec, expr,
│   │                     #   call, natives — evaluates against doge-runtime directly
│   └── doge-lsp/         # language server (doge lsp): thin LSP glue over the
│                         #   doge-compiler front end + completion engine
├── examples/             # .doge example programs (double as integration tests)
└── docs/                 # this documentation
```

All crates share one version through `[workspace.package]`; the build cache is
salted with that version, a codegen-revision constant, and a hash of the
`doge-runtime` source, so a runtime change never serves a stale cached binary. The
generated per-script crate depends on `doge-runtime` by path in a dev checkout, and
by the compiler's own published version once `doge` is installed — so a
`cargo install`ed binary compiles scripts without the source tree beside it.

## 3. Runtime model (`doge-runtime`)

- `enum Value { Int(i64), Float(f64), Str(Rc<str>), Bool(bool), None, List(Rc<RefCell<Vec<Value>>>), Dict(Rc<RefCell<OrderedMap>>), Func(…), Object(…) }` — `OrderedMap` is an insertion-ordered string→`Value` map, so dict iteration and printing are deterministic.
- All fallible operations return `Result<Value, DogeError>`; generated code threads
  `?` through, and `pls`/`oh no` compiles to a `match` on the block's `Result`.
  No panics in the happy path; no `unsafe` anywhere.
- `bark` is a runtime print with doge-friendly `Display` formatting of values.
- Stdlib modules (`nerd`, `strings`, `fetch`, `env`, `howl`, `pack`, `json`, `dson`) are Rust
  functions in the runtime, one per member, named `{module}_{member}` (`nerd_sqrt`,
  `fetch_read`).
- Objects are `Rc<RefCell<ObjectData>>`: a class id, the class name, and a field
  map. `attr_get`/`attr_set` read and write fields, and a generated dispatcher
  routes each method call to the right runtime call. Inheritance
  (`many Child much Parent:`) is flattened at codegen — the runtime `ObjectData`
  has no parent pointer; an instance carries only its own class id.
- List and dict methods (`xs.append(1)`, `d.keys()`) are not modules: the
  generated `call_method` dispatcher forwards any non-`Object` receiver to
  `builtin_method` in the runtime (`methods/`), the collection counterpart of
  the object dispatcher.
- Concurrency (`pack` module) keeps the single-threaded `Rc`/`RefCell` model by
  copying at the thread boundary rather than sharing. A `Value` is `!Send`, so
  `pack.rs` defines `Packed` — an owned, `Rc`-free mirror that *is* `Send` —
  plus `pack_value`/`unpack_packed` to deep-copy across. A pup (thread) gets its
  own single-threaded world: `pack.zoom` snapshots the callee, arguments, and
  globals into `Packed`, spawns a thread, and the pup rebuilds fresh `Value`s and
  runs. `Value::Pup`/`Value::Bowl` are opaque handles like `Value::Socket`; a bowl
  (channel) carries `Packed` over `std::sync::mpsc` and is shared (not copied)
  across the boundary, a socket transfers. No locks on the value hot path, no
  `unsafe`, and every misuse is a catchable error. Codegen emits a `pup_entry`
  trampoline + `snapshot_env` for the compiled path; `doge-interp` rebuilds a fresh
  interpreter over the same `Arc<Program>` on the pup's thread, so both engines
  behave identically (the examples parity suite covers `pack_*.doge`).

## 4. Codegen

Top-level state lives in a generated `Env` struct (the line tracker, the recursion
depth, and one field per top-level bound name), threaded by `&mut` through `run` and
every function, so a function can read and reassign top-level names. `main` builds the
`Env`, calls `run`, and turns an uncaught error into a doge-flavored message.
Generated from `such age = 7` / `bark "age is " + str(age)`:

```rust
#![allow(warnings)]
use doge_runtime::*;

struct Env {
    cur_line: u32,
    depth: usize,
    v_age: Value,
}

fn main() -> std::process::ExitCode {
    let mut env = Env { cur_line: 0, depth: 0, v_age: Value::None };
    match run(&mut env) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("very error. much broken.\n\n  examples/hello.doge:{}\n  {e}", env.cur_line);
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(env: &mut Env) -> DogeResult<()> {
    env.cur_line = 1;
    env.v_age = Value::Int(7i64);
    env.cur_line = 2;
    let _ = bark(&add(Value::str("age is "), to_str(&env.v_age.clone()))?);
    Ok(())
}
```

Rules that make the generated Rust compile by construction (the "Rust never leaks"
rule, [ERRORS.md](ERRORS.md)):

- Hoisted names become fields or locals. Every `such`/`so` name, `for` variable,
  and `oh no` error name is bound once: as an `Env` field at the top level, or as a
  `let mut v_<name>: Value = Value::None;` inside the function that owns it. A binding
  never executed holds `none`.
- Every identifier carries a `v_` prefix, so a Doge name that is a Rust keyword
  (`match`, `loop`, `type`) can never collide. The prefix is invisible to the user.
- `env.cur_line` is set before each statement, giving an uncaught error a precise
  Doge source line without any Rust backtrace.
- Each function is a wrapper + body pair. `f_<name>` counts the call against the
  recursion limit (`enter_call`), calls `b_<name>`, then `exit_call`s on every exit
  path, so the depth is always restored, even when the body returns via `?`. User
  arguments are passed before the shared `&mut env` so their borrows stay disjoint.
- Loops are labeled (`'lN`) and try blocks are labeled (`'pN`). A `pls` body is a
  labeled block; a fallible call inside it breaks to `'pN` (so `oh no` catches it)
  instead of `?`-ing out of the function, and `bork`/`continue` target the innermost
  loop label, which lets a `bork` cross an enclosing `pls`. `bonk` raises via
  `bonk_error`, and `error_value` turns a caught error into the bound `Str`.
- All logic lives in `doge-runtime`. Codegen emits only wiring (`add`, `eq`,
  `index_get`, `iter_value`, `bark`, `range`, `enter_call`, and so on), each fallible
  call threaded with `?` or the labeled-break form. `and`/`or` become
  short-circuiting Rust block expressions that yield a Bool.
- Each object becomes a constructor plus a method pair per method. `n_<id>`
  builds a `Value::object(id, "Shibe")` and runs `init`; every method is an
  `mf_`/`mb_` wrapper+body pair with `self` as its first parameter (so it counts
  against the recursion limit like any call). A single `call_method(recv, name,
  args, env)` dispatcher matches `(class_id, method_name)`, checks the argument
  count, and calls the right `mf_`. Field access is `attr_get`/`attr_set`. Class
  ids are program-wide, so a module's objects import and construct by member
  (`utils.Shibe(…)`) just like its functions.
- Inheritance is compile-time flattening. `many Child much Parent:` gives `Child`
  a dispatcher arm for every method up its ancestry — an inherited method's arm
  targets the ancestor's `mf_`, an override targets the child's own. A child with
  no `init` constructs through the nearest ancestor's, and `super.method(args)`
  resolves statically to the ancestor `mf_`, called with the current `self`. The
  parent must live in the same file; the checker rejects an unknown parent or a
  cycle before codegen.
- A stdlib member call emits its runtime function. `nerd.sqrt(16)` becomes
  `nerd_sqrt(&Value::Int(16i64))`; a constant like `nerd.pi` inlines as
  `Value::Float(std::f64::consts::PI)`. Arity and unknown-member errors are caught
  at compile time from a module table that mirrors the runtime.
- The script's source lines are embedded as `static LINES`, so an uncaught
  error prints the offending line under `path:line` with no Rust backtrace
  ([ERRORS.md](ERRORS.md)).

Nested functions are closures. A capture analysis over the AST decides which
locals a nested function reads or writes; those become shared `Rc<RefCell<Value>>`
cells, and the nested function is emitted as a `c_`/`cb_` pair that takes its
captured cells as leading parameters. Functions are first-class values: every
top-level function, closure, builtin, and module function has a numeric `fn_id`,
and a `Value::Function` carries that id plus its captured cells. A call through a
value routes through a generated `call_function(fn_id, …)` dispatcher, mirroring
the method dispatcher; direct calls by name stay static and keep their
compile-time arity check. A class name used as a value is the same machinery: each
class gets one extra `call_function` arm that runs its constructor, and the name
materializes as a `Value::Class` over that `fn_id` — a distinct value kind (prints
`<class Name>`, type `Class`) that `callee_function` unwraps just like a function,
so the whole indirect-call path is reused unchanged.

A method read off a value (`such f = a.speak`) has no `fn_id` — dispatch is
name-based — so it is a `Value::BoundMethod` carrying the receiver and the method
name. A bare `obj.name` value read emits `attr_get_or_bind`, which returns a field
if there is one and otherwise binds the method (a generated `class_has_method` gate
for object receivers, the runtime's collection-method table for List/Dict). Indirect
calls go through a `call_value` shim: a bound method routes straight back to
`call_method`, everything else to `call_function`. Both engines share this, so a
stored method behaves the same compiled or interpreted.
