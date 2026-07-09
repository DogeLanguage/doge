# Doge Standard Library

Builtins are always in scope; modules are pulled in with `so <name>` at the top of
the script (import syntax and rules: [SYNTAX.md](SYNTAX.md) §9). All of it is
implemented in Rust inside `doge-runtime`.

## Builtins (no import needed)

| Builtin | Meaning |
|---|---|
| `len(x)` | character count of a Str, element count of a List/Dict |
| `str(x)` | convert to Str (the same display form `bark` prints) |
| `int(x)` | convert to Int |
| `float(x)` | convert to Float |
| `range(n)` / `range(a, b)` | a List of Ints `0 … n-1`, or `a … b-1` |

`range` bounds must be Ints, and the List is empty when the end is not past the
start.

## Modules

v1 ships three stdlib modules. There is no `math` module; the math module is
`nerd`.

| Module | Members |
|---|---|
| `nerd` | `abs`, `sqrt`, `floor`, `ceil`, `round`, `min`, `max`, `pow`; constants `pi`, `e` |
| `strings` | `beeg` (uppercase), `smoll` (lowercase), `trim`, `split`, `join`, `contains`, `replace` |
| `lists` | `push`, `pop`, `sort`, `reverse`, `contains` (`push`/`sort`/`reverse` change the list in place and give back `none`) |

A member is either a function, like `nerd.sqrt(16)` or `strings.beeg("wow")`, or a
constant (`nerd.pi`). Arity and unknown-member errors are caught at compile time
from a module table in the compiler that mirrors the runtime.

Importing other `.doge` files is a later milestone.
