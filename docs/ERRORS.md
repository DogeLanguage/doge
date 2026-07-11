# Doge Error Message Style

How Doge talks to the user when something goes wrong. The `pls`/`oh no`/`bonk`
language constructs themselves are documented in [SYNTAX.md](SYNTAX.md) §7.

Errors are doge-flavored but always carry real information (file, line, caret):

```
very error. much confuse.

  examples/hello.doge:4
    bark "hello" + 5
                 ^ cannot + a Str and an Int

such fix: turn the Int into a Str first, e.g. str(5)
```

An uncaught runtime error reads the same way, showing the source line it came
from (embedded at compile time, so there is never a Rust backtrace):

```
very error. much broken.

  examples/divide.doge:3
    bark a // 0
  cannot // by zero
```

Tone: meme in the framing, precision in the content. Never sacrifice clarity for
the joke.

A *caught* error (`pls`/`oh no`) is not just this text: `oh no err!` binds a
structured `Error` value carrying the same category, message, and raise location
the report above shows — `err.type` / `err.message` / `err.file` / `err.line`.
The value's fields and `bonk err` re-raise semantics live in
[SYNTAX.md](SYNTAX.md) §7.

Rules:

- One issue at a time, never a wall of errors.
- Always file, line, and caret pointing at the offending spot.
- A concrete fix hint (`such fix: …`) whenever one exists.
- Compile errors get friendly memed framing, e.g. a missing `wow` is
  `"very incomplete. such missing wow. (did the script end early?)"` and a tab in
  leading whitespace is `"very tab. much confuse."`.
- Rust never leaks: the user must never see rustc
  output, Rust type names, or a Rust panic. If `rustc` rejects generated code, or
  generated code panics at runtime, that is a Doge compiler bug and is reported as
  one (`"very bug. much sorry. pls report: <url>"` plus the internal log), never
  as raw Rust errors.

## Import diagnostics

Importing another `.doge` file ([SYNTAX.md](SYNTAX.md) §9) has its own errors,
each pointing at the offending `so` line in the file that wrote it:

- **Unknown module** — `very import. much unknown.` — `so nope` names neither a
  built-in module nor a `nope.doge` next to the importing file. The hint names the
  file to create, or the built-in modules.
- **Loose statement in a module** — `very loose. much module.` — a module file
  only defines things; a statement that would run at import time is rejected with
  a "wrap it in a function, or move it to your main script" hint.
- **Object in a module** — `very object. much soon.` — `many` in a module is not
  yet importable; the hint points to defining it in the main script for now.
- **Import cycle** — `very loop. much import.` — files that import each other in a
  loop; the message spells out the chain (`a → b → a`).
- **Stdlib shadow** — `very shadow. much confuse.` — a user file named like a
  built-in module (`nerd.doge`) can never be reached; the hint is to rename it.

An uncaught runtime error inside an imported module reports *that module's* file
and line, not the entry script's.

## Function-header and call diagnostics

Parameters with defaults, keyword arguments, and variadics ([SYNTAX.md](SYNTAX.md)
§6) each carry their own diagnostic:

- **Arity out of range** — `very args. much wrong.` — a call supplies too few or
  too many arguments. The message states the accepted span: `greet takes 1 to 2
  arguments, got 0`, a plain `add2 takes 2 arguments, got 1` for a fixed header, or
  `party takes at least 1 argument, got 0` when a variadic makes the top unbounded.
  The hint echoes the call shape (`greet(name, mood = …, many rest)`). A direct
  call is caught at compile time; a call through a value at run time.
- **Required after default** — `very order. much default.` — a parameter with no
  default follows one with a default; the hint is to move defaulted parameters to
  the end.
- **Variadic not last** — `very rest. much greedy.` — `many rest` is followed by
  another parameter; it must be the final one.
- **Non-literal default** — `very default. much dynamic.` — a default value is not
  a literal; the hint lists the allowed forms (`0`, `"hi"`, `true`, `none`, `[ ]`).
- **Duplicate parameter** — `very twice. much name.` — a parameter (or the
  variadic) repeats a name in the same header.
- **Keyword ordering** — `very order. much muddle.` — a positional argument follows
  a keyword one.
- **Repeated keyword** — `very keyword. much repeat.` — the same keyword name is
  passed twice at a call, or a keyword names a parameter already filled
  positionally.
- **Unknown keyword** — `very keyword. much unknown.` — a keyword name matches no
  parameter (or names the variadic, which cannot be filled by keyword); the hint
  shows the call shape.
- **Keyword where none is allowed** — `very keyword. much dynamic.` — a keyword
  argument is passed to a method, a stored function value, or a builtin; the hint
  is to pass it positionally or call the function by a name doge knows.
