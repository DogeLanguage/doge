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

## Collection methods (no import needed)

Lists and dicts carry their own methods, called on the value with `.` — no
import, like a field access that runs. A method on a value that has none, or an
unknown method name, is a catchable error (`pls`/`oh no`), as is a wrong argument
count.

```doge
such xs = [3, 1, 2]
xs.append(4)          # [3, 1, 2, 4]
xs.sort()             # [1, 2, 3, 4]
bark xs.pop()         # 4
```

### List methods

| Method | Returns | Meaning |
|---|---|---|
| `append(item)` | `none` | add `item` to the end |
| `pop()` | the element | remove and return the last element (empty List is a catchable error) |
| `insert(i, item)` | `none` | insert before index `i`; `i` may be negative (from the end) and `i == len()` appends |
| `remove(item)` | `none` | remove the first element equal to `item` (not found is a catchable error) |
| `index_of(item)` | `Int` | index of the first element equal to `item` (not found is a catchable error) |
| `contains(item)` | `Bool` | whether any element equals `item` |
| `sort()` | `none` | sort in place; elements must be all Ints/Floats or all Strs |
| `reverse()` | `none` | reverse in place |
| `clear()` | `none` | remove every element |

`append`, `insert`, `remove`, `sort`, `reverse`, and `clear` change the list in
place and give back `none`.

### Dict methods

| Method | Returns | Meaning |
|---|---|---|
| `keys()` | `List` | the keys, in insertion order |
| `values()` | `List` | the values, in insertion order |
| `items()` | `List` | one `[key, value]` List per entry, in insertion order |
| `has(key)` | `Bool` | whether `key` is present |
| `remove(key)` | the value | remove `key` and return its value (missing key is a catchable error) |
| `clear()` | `none` | remove every entry |

Dicts are **insertion-ordered**: `keys()`, `values()`, `items()`, and printing a
dict all follow the order keys were first inserted. Assigning to an existing key
updates its value but keeps its original position.

Methods are not first-class values — `such f = xs.append` is a catchable runtime
error, since `xs.append` reads a (non-existent) field before the call.

## Modules

v1 ships two stdlib modules. There is no `math` module; the math module is
`nerd`.

| Module | Members |
|---|---|
| `nerd` | `abs`, `sqrt`, `floor`, `ceil`, `round`, `min`, `max`, `pow`; constants `pi`, `e` |
| `strings` | `beeg` (uppercase), `smoll` (lowercase), `trim`, `split`, `join`, `contains`, `replace` |

A member is either a function, like `nerd.sqrt(16)` or `strings.beeg("wow")`, or a
constant (`nerd.pi`). Arity and unknown-member errors are caught at compile time
from a module table in the compiler that mirrors the runtime.

Importing other `.doge` files is a later milestone.
