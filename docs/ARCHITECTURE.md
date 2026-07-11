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

A `so <name>` import resolves to a built-in module (`nerd`/`strings`/`lists`) or,
failing that, the user file `<name>.doge` next to the importer. The loader parses
every reachable file into one `Program`; the whole downstream pipeline works on
that. Codegen keeps every file's top-level names in one flat Rust namespace by
mangling with a per-file id: the entry (file 0) keeps its plain `f_`/`v_` scheme,
so single-file output is unchanged, and a module (file N) carries its id right
after the prefix (`f1_square`, `g1_ANSWER`) — a digit can't start a doge
identifier, so these never collide with the entry's names. A multi-file program
also embeds a per-file source table so an uncaught runtime error reports the
module and line it actually came from.

## 2. Crate layout

```
doge/
├── Cargo.toml            # workspace
├── crates/
│   ├── doge-cli/         # `doge` binary: bark/build/check subcommands, build cache
│   ├── doge-compiler/    # lexer, parser, AST, checks, Rust codegen
│   └── doge-runtime/     # Value enum, operators, builtins, stdlib (precompiled)
├── examples/             # .doge example programs (double as integration tests)
└── docs/                 # this documentation
```

All crates stay at v0.1.0; the build cache is salted with the compiler version
instead of bumping crate versions.

## 3. Runtime model (`doge-runtime`)

- `enum Value { Int(i64), Float(f64), Str(Rc<str>), Bool(bool), None, List(Rc<RefCell<Vec<Value>>>), Dict(Rc<RefCell<HashMap<…>>>), Func(…), Object(…) }`
- All fallible operations return `Result<Value, DogeError>`; generated code threads
  `?` through, and `pls`/`oh no` compiles to a `match` on the block's `Result`.
  No panics in the happy path; no `unsafe` anywhere.
- `bark` is a runtime print with doge-friendly `Display` formatting of values.
- Stdlib modules (`nerd`, `strings`, `lists`) are Rust functions in the runtime,
  one per member, named `{module}_{member}` (`nerd_sqrt`, `strings_beeg`).
- Objects are `Rc<RefCell<ObjectData>>`: a class id, the class name, and a field
  map. `attr_get`/`attr_set` read and write fields, and a generated dispatcher
  routes each method call to the right runtime call.

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
  count, and calls the right `mf_`. Field access is `attr_get`/`attr_set`.
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
compile-time arity check.
