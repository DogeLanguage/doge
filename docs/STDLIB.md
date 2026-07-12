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
| `gib()` / `gib("prompt")` | read one line from standard input as a Str, or `none` at end of input |

`range` bounds must be Ints, and the List is empty when the end is not past the
start.

`gib` reads a single line of user input. An optional prompt (which must be a Str)
is printed first, without a trailing newline, so the user types on the same line;
`gib()` with no prompt just reads. The returned Str has its trailing newline
stripped, and at end of input (Ctrl-D, or a closed pipe) `gib` returns `none`:

```doge
such name = gib("who is a good dog? ")
bark "much hello {name}"
```

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

v1 ships four stdlib modules. There is no `math` module; the math module is
`nerd`.

| Module | Members |
|---|---|
| `nerd` | `abs`, `sqrt`, `floor`, `ceil`, `round`, `min`, `max`, `pow`; constants `pi`, `e` |
| `strings` | `beeg` (uppercase), `smoll` (lowercase), `trim`, `split`, `join`, `contains`, `replace` |
| `fetch` | `read`, `write`, `append`, `exists`, `delete` — file I/O |
| `env` | `args`, `get` — command-line arguments and environment variables |

A member is either a function, like `nerd.sqrt(16)` or `strings.beeg("wow")`, or a
constant (`nerd.pi`). Arity and unknown-member errors are caught at compile time
from a module table in the compiler that mirrors the runtime.

### `fetch` — file I/O

Every path (and, for writes, the text) must be a Str; anything else is a catchable
`TypeError`. Every OS failure — a missing file, a permission problem, bytes that
are not valid text — is a catchable `IOError` (`err.type == "IOError"`), never a
crash.

| Member | Returns | Meaning |
|---|---|---|
| `read(path)` | `Str` | the whole file as text (missing file or non-text bytes are an `IOError`) |
| `write(path, text)` | `none` | replace the file's contents with `text`, creating it if needed |
| `append(path, text)` | `none` | add `text` to the end of the file, creating it if needed |
| `exists(path)` | `Bool` | whether anything exists at `path` |
| `delete(path)` | `none` | remove the file (a missing file is an `IOError`) |

```doge
so fetch
fetch.write("notes.txt", "much wow")
fetch.append("notes.txt", "\nsuch file")
bark fetch.read("notes.txt")
```

### `env` — arguments and environment

| Member | Returns | Meaning |
|---|---|---|
| `args()` | `List` of `Str` | the script's command-line arguments, excluding the program name |
| `get(name)` | `Str` or `none` | the value of environment variable `name`, or `none` when it is unset |

`env.args()` reflects the arguments after the script when run with
`doge bark script.doge alpha beta` (`["alpha", "beta"]`), or the arguments to a
standalone `doge build` binary. `env.get(name)` needs a Str name; a missing or
non-text variable reads back as `none`.

A `so <name>` import that is not a stdlib module resolves to the user file
`<name>.doge` next to the importer; see [SYNTAX.md](SYNTAX.md) §9.
