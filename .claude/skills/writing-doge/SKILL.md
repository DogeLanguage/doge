---
name: writing-doge
description: Write, edit, review, or debug programs in the Doge language (`.doge` files). Doge is an indentation-based scripting language whose keywords come from doge-speak — `such`, `much`, `many`, `wow`, `bark`, `pls`/`oh no`, `bonk`, `so`. Use this skill whenever you author or modify a `.doge` script, add an `examples/*.doge` test, or hit a Doge syntax/runtime error — even if the request just says "write a Doge program" or names a `.doge` file without mentioning the syntax. An LLM's Python instincts get Doge wrong (there is no `def`/`class`/`print`/`try`, scripts must end in `wow`), so consult this skill before writing the first line rather than guessing.
---

# Writing Doge

Doge is a dynamically typed, indentation-based scripting language. It looks Python-ish but its keywords are doge-speak and several rules differ sharply — **do not** transliterate Python. This cheat sheet is enough to write most programs correctly. For exact stdlib signatures see `references/stdlib.md`; the authoritative spec is `docs/SYNTAX.md`, `docs/GRAMMAR.md`, `docs/STDLIB.md`, `docs/CLI.md`, `docs/ERRORS.md` (repo-relative).

## The rules that bite first

These are the ones Python instinct gets wrong. Internalize them before writing:

- **Every script ends with `wow`** on its own line at top level. Forgetting it is a compile error.
- **`def`, `class`, `print`, `try`/`except` do not exist.** Use `such` (function), `many` (object), `bark` (print), `pls`/`oh no` (try/catch).
- **Function and object definitions close with `wow`** aligned under the definition. Control-flow blocks (`if`/`elif`/`else`/`for`/`while`/`pls`/`oh no`) close by **dedent only — no `wow`**.
- **Declare before use:** `such x = ...` introduces a name; assigning to an undeclared name is a compile error (this catches typos). Reassign a declared name with plain `x = ...` or the flavored `very x = ...`.
- **Indent with spaces, never tabs.** A tab in leading whitespace is a compile error.
- **`pls` opens its block bare (no `:`)**; the catch header is `oh no name!` — ending in `!`, not `:`.
- **`/` always returns a Float** (`5 / 2` is `2.5`); use `//` for integer division. Comparisons do not chain: write `1 < x and x < 10`.

## Keyword map

| Doge | Means | Doge | Means |
|---|---|---|---|
| `such x = v` | declare variable (`let`) | `bark e` | print |
| `such f much a, b:` … `wow` | define function | `bonk e` | raise (catchable) |
| `such f:` … `wow` | function, no params (`much` omitted) | `amaze cond` | assert (opt. `, msg`) |
| `many Name:` … `wow` | define object/class | `pls` … `oh no err!` … | try / catch |
| `many Child much Parent:` | inherit | `bork` | break |
| `much` | intro params, or parent class, or trailing collector | `continue` | continue |
| `so name` | import module | `return e` | return (`none` if omitted) |
| `so NAME = v` | constant (no reassign) | `self` | receiver inside a method |
| `very x = v` | reassign (flavored `x = v`) | `super.m(a)` | call parent method |

Universal keywords kept as-is: `if elif else for while in return continue and or not true false none`.
`in` is both the `for` iterable introducer and the membership operator (`x in xs`).

## Core forms

```doge
so nerd                          # import (top level only)
so GREETING = "much hello"       # constant

such greet much name, mood = "happy":     # default must be a literal
    return "much hello {name}, very {mood}"   # {expr} interpolation
wow

such shibes = ["kabosu", "cheems"]
such ages = {"kabosu": 18}       # dicts are insertion-ordered

for name in shibes:
    if name == "walter":
        bork                     # break; blocks close by dedent, no wow
    bark greet(name)

pls
    such n = int(gib("age? "))   # gib reads a line; int/str/float/dec/bytes/len/range are builtins
oh no err!
    bark "very error: " + err    # err is a structured Error; str()s to its message
```

Objects, inheritance, `self`, `super`:

```doge
many Animal:
    such init much name:         # `init` is the constructor
        self.name = name         # fields appear on first assignment
    wow
    such speak:
        return self.name + " makes a sound"
    wow
wow

many Shibe much Animal:
    such speak:                  # override
        return self.name + " says bork"
    wow
wow

such s = Shibe("kabosu")         # Shibe(...) builds an instance; arity checked vs init
bark s.speak()
```

## Semantics worth knowing

- **Numbers.** `Int` is arbitrary precision (never overflows). `/` → Float, `//` → integer, `**` → power (right-assoc). Int and Float mix freely. `Decimal` (exact, via `dec("19.99")`, no literal) is for money — mixes with Int but **mixing Decimal with Float is a catchable `TypeError`**; convert one side.
- **Strings** are character-indexed (`"héllo"[1]` is `"é"`); `len` counts characters. `Bytes` (via `bytes(x)`) is the byte-indexed counterpart. Negative indices and slices work: `xs[-1]`, `xs[1:3]`, `xs[::-1]`.
- **Interpolation** `"{expr}"` holes render any expression with its display form. Write `\{` for a literal brace. Regex `\d` must be `\\d` in a string.
- **Truthiness** follows Python: `0`, `""`, `[]`, `{}`, `none`, `false` are falsy. `and`/`or` short-circuit and yield a **Bool**, not the operand.
- **Membership:** `x in xs` (list element / dict key / substring), `x not in xs`.
- **Ternary:** `a if cond else b` (the `else` is required).
- **Destructuring:** `such a, b = [1, 2]`, swap `a, b = b, a`, rest `such head, many rest = xs`.
- **Augmented assignment** covers every arithmetic and bitwise operator (`+= -= *= /= //= %= **= &= |= ^= <<= >>=`) on any assignable target — `count += 1`, `xs[i] *= 2`, `dog.age += 1`. The target must already be declared (it is a reassignment), and a `so` constant cannot be its target.
- **Functions are values** (`such g = greet`, then `g(...)`); so are classes and bound methods (`such push = xs.append`).
- **Keyword arguments** (`greet("k", mood = "sleepy")`) work **only** when Doge knows the function at compile time: a top-level function, a constructor, or an imported module function. On a **method, a stored function value, or a builtin, pass positionally** — a keyword there is a compile error.
- **Errors are structured.** `oh no err!` binds `err` with `.type` `.message` `.file` `.line`. Types: `TypeError`, `DivisionByZero`, `Overflow`, `IndexOutOfBounds`, `KeyError`, `ValueError`, `IOError`, `AttrError`, `Bonk`, `AssertError`, `RecursionLimit`. Re-raise unchanged with `bonk err`.
- **Collection methods** are called on the value, not imported: `xs.append(1)`, `xs.sort()`, `d.keys()`, `d.has(k)`. They are **not** first-class (`such f = xs.append` on a plain list access errors — but a bound method read off the value is; see `references/stdlib.md`).
- **Imports & modules** live at the top level. Stdlib: `nerd` (math — there is no `math`), `strings`, `hunt` (regex), `fetch` (files), `env`, `howl` (net/HTTP), `pack` (threads), `json`, `dson`, `nap` (time), `roll` (random), `chase` (subprocess). A `so name` that matches no stdlib loads the sibling `name.doge`; a module file holds **only** definitions. Full member lists: `references/stdlib.md`.

## Running, checking, testing

Doge transpiles to Rust and caches the binary, so first run pays a compile cost, then it's instant.

- `doge bark script.doge [args…]` — compile (cached) and run; `args…` reach the script via `env.args()`.
- `doge check script.doge` — parse + semantic checks only, no build. **Fastest way to validate syntax.**
- `doge fmt script.doge` — format in place to canonical style (four-space indent); `--check` for CI.
- `doge test <file-or-dir>` — runs every top-level zero-arg function whose name starts with `test`, asserting with `amaze`:

```doge
such test_addition:
    amaze 1 + 1 == 2
wow
```

## Workflow for a new program or feature

1. **Sketch the shape first** in your head: which top-level functions/objects, which stdlib modules. Grep `examples/` for a `.doge` that already does something similar and match its style — the examples are the living style guide.
2. **Write the script.** Keep `wow` discipline: every `such f`/`many N` and the file itself. Prefer the flavored construct (`bark`, `pls`/`oh no`, `amaze`) over hand-rolling.
3. **Validate:** run `doge check` for fast syntax feedback, then `doge bark` to run it. If you touched the language itself (this repo), a feature ships an `examples/*.doge` that runs as an integration test — a `.out` sibling asserts its stdout.
4. **Read the error, don't guess.** Doge errors carry file, line, a caret, and a `such fix:` hint. They mean exactly what they say. If you ever see raw `rustc`/Rust output, that is a compiler bug, not your script's fault.

## When to read more

- Exact signatures for every builtin, collection method, and stdlib module member → `references/stdlib.md`.
- A subtle semantic (closure capture, slice clamping, `super` rules, pack/thread copy semantics, DSON octal numbers) → the authoritative `docs/SYNTAX.md` and `docs/STDLIB.md`.
- Working examples of every feature → the repo's `examples/*.doge`.
