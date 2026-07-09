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
