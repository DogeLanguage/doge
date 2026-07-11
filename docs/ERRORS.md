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
