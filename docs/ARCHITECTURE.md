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

A `so <name>` import resolves to a built-in module (`nerd`/`strings`) or,
failing that, the user file `<name>.doge` next to the importer. The loader parses
every reachable file into one `Program`; the whole downstream pipeline works on
that. Codegen keeps every file's top-level names in one flat Rust namespace by
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

## 2. Crate layout

```
doge/
├── Cargo.toml            # workspace ([workspace.package] shares version/edition)
├── rust-toolchain.toml   # pinned stable toolchain (rustfmt + clippy)
├── crates/
│   ├── doge-cli/         # `doge` binary: main + build + cache; build.rs salts the
│   │                     #   cache key with a hash of the doge-runtime source
│   ├── doge-compiler/    # each pass is a directory module:
│   │                     #   lexer/ (mod, scan, strings)
│   │                     #   parser/ (mod, stmt, expr)
│   │                     #   check/ (mod, stmt, scopes)
│   │                     #   codegen/ (mod, program, names, analysis, callable,
│   │                     #             stmt, expr, calls, dispatch)
│   │                     #   modules/ (mod, diag)  — the import loader
│   │                     #   ast/ (mod, dump)      — nodes + shared AST walker
│   │                     #   plus keywords, token, builtins, stdlib, diagnostics
│   ├── doge-runtime/     # Value enum, ops/ (arith, compare, index), methods/
│   │                     #   (list, dict), builtins, objects, stdlib/ (nerd, strings)
│   └── doge-interp/      # tree-walking interpreter over the checked AST (doge repl):
│                         #   analyze (fn ids + captures + class table), exec, expr,
│                         #   call, natives — evaluates against doge-runtime directly
├── examples/             # .doge example programs (double as integration tests)
└── docs/                 # this documentation
```

All crates share one version through `[workspace.package]`; the build cache is
salted with that version, a codegen-revision constant, and a hash of the
`doge-runtime` source, so a runtime change never serves a stale cached binary.

## 3. Runtime model (`doge-runtime`)

- `enum Value { Int(i64), Float(f64), Str(Rc<str>), Bool(bool), None, List(Rc<RefCell<Vec<Value>>>), Dict(Rc<RefCell<OrderedMap>>), Func(…), Object(…) }` — `OrderedMap` is an insertion-ordered string→`Value` map, so dict iteration and printing are deterministic.
- All fallible operations return `Result<Value, DogeError>`; generated code threads
  `?` through, and `pls`/`oh no` compiles to a `match` on the block's `Result`.
  No panics in the happy path; no `unsafe` anywhere.
- `bark` is a runtime print with doge-friendly `Display` formatting of values.
- Stdlib modules (`nerd`, `strings`) are Rust functions in the runtime,
  one per member, named `{module}_{member}` (`nerd_sqrt`, `strings_beeg`).
- Objects are `Rc<RefCell<ObjectData>>`: a class id, the class name, and a field
  map. `attr_get`/`attr_set` read and write fields, and a generated dispatcher
  routes each method call to the right runtime call. Inheritance
  (`many Child much Parent:`) is flattened at codegen — the runtime `ObjectData`
  has no parent pointer; an instance carries only its own class id.
- List and dict methods (`xs.append(1)`, `d.keys()`) are not modules: the
  generated `call_method` dispatcher forwards any non-`Object` receiver to
  `builtin_method` in the runtime (`methods/`), the collection counterpart of
  the object dispatcher.

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
