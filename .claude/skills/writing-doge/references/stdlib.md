# Doge stdlib — condensed reference

Signatures for every builtin, collection method, and stdlib module member. Names in
doge-speak are unguessable, so look them up here rather than inventing them. This
mirrors `docs/STDLIB.md`; read that for full semantics and edge cases. Every failure
below is a **catchable** error (`pls`/`oh no`), never a crash.

## Builtins (always in scope, no import)

| Call | Returns / meaning |
|---|---|
| `len(x)` | char count of Str, byte count of Bytes, element count of List/Dict |
| `str(x)` | Str (the display form `bark` prints) |
| `int(x)` | Int: parse any-size whole from Str; truncate Float/Decimal toward zero; Bool→0/1 |
| `float(x)` | Float |
| `dec(x)` | exact Decimal from Str/Int; from Float via its shortest decimal form |
| `bytes(x)` | Bytes: UTF-8-encode a Str, or a List of Ints 0–255 |
| `range(n)` / `range(a, b)` | List of Ints `0…n-1` or `a…b-1` (bounds must be Ints) |
| `gib()` / `gib("prompt")` | read one input line as Str (prompt printed first, no newline); `none` at EOF |

## List methods (call on the value: `xs.append(1)`)

| Method | Returns | Meaning |
|---|---|---|
| `append(item)` | `none` | add to end |
| `pop()` | element | remove+return last (empty is an error) |
| `insert(i, item)` | `none` | insert before `i` (neg ok; `i == len()` appends) |
| `remove(item)` | `none` | remove first equal (not found is an error) |
| `index_of(item)` | `Int` | index of first equal (not found is an error) |
| `contains(item)` | `Bool` | any element equal |
| `sort()` | `none` | in place; all Ints/Floats or all Strs |
| `reverse()` | `none` | in place |
| `clear()` | `none` | remove all |

## Dict methods

| Method | Returns | Meaning |
|---|---|---|
| `keys()` / `values()` | `List` | in insertion order |
| `items()` | `List` | one `[key, value]` List per entry |
| `has(key)` | `Bool` | key present |
| `remove(key)` | value | remove+return (missing is an error) |
| `clear()` | `none` | remove all |

Dicts are insertion-ordered; `for k in d` walks keys in that order. Methods called on
the value are fine; a **bound method taken as a value** (`such push = xs.append`) is
first-class and mutates `xs` when called.

## Bytes methods

`hex()` → lowercase hex Str · `decode()` → UTF-8 Str (invalid UTF-8 is a `ValueError`).
`b[i]` is an Int 0–255; `len(b)` counts bytes; slicing yields Bytes.

## Modules (import with `so <name>`)

### `nerd` — math (there is no `math` module)
`abs`, `sqrt`, `floor`, `ceil`, `round`, `min`, `max`, `pow`; constants `nerd.pi`, `nerd.e`.

### `strings`
`beeg` (UPPER), `smoll` (lower), `trim`, `split(s, sep)`, `join(list, sep)`,
`contains(s, sub)`, `replace(s, old, new)`.

### `hunt` — regex (pattern is arg 1, text is arg 2; both Str)
`test(pat, text)`→Bool · `find(pat, text)`→Str|none · `find_all(pat, text)`→List ·
`groups(pat, text)`→List|none (group 0 = whole match) · `replace(pat, text, repl)`→Str
(`$1`, `${name}` backrefs). Write `\\d`/`\\w` in Doge string literals. Bad pattern → `ValueError`.

### `fetch` — files, dirs, paths
`read(path)`→Str · `write(path, text)` · `append(path, text)` · `read_bytes(path)`→Bytes ·
`write_bytes(path, bytes)` · `exists(path)`→Bool · `delete(path)` · `list(path)`→List (sorted) ·
`make_dir(path)` · `remove_dir(path)` · `rename(from, to)` · `copy(from, to)` ·
`stat(path)`→Dict `{size, modified, is_dir}` · `join(a, b)`→Str · `basename(path)`→Str · `ext(path)`→Str.
OS failures are `IOError`; `join`/`basename`/`ext` are pure string ops.

### `env`
`args()`→List of Str (script args, program name excluded) · `get(name)`→Str|none.

### `howl` — TCP sockets + HTTP client
`listen(host, port)`→Socket (port 0 = OS-chosen) · `connect(host, port)`→Socket ·
`accept(listener)`→Socket · `port(sock)`→Int · `send(conn, text)` · `recv(conn, max)`→Str|none ·
`recv_line(conn)`→Str|none · `close(sock)` · `get(url)`→`{status, body}` · `post(url, body)`→`{status, body}`.
Network failures are `IOError`; a non-2xx HTTP response is a normal `{status, body}` Dict, not an error.

### `pack` — threads (pups) and channels (bowls)
`zoom(f, args)`→Pup (runs `f(args…)` on a new thread) · `fetch(pup)`→result (blocks; re-raises the pup's error) ·
`bowl()`→Bowl · `drop(bowl, value)` · `sniff(bowl)`→value (blocks, FIFO).
Each pup gets a deep copy of args/captures/top-level snapshot — no shared mutable state; a bowl is shared.
`fetch` a pup only once. The script does not wait for un-fetched pups at exit.

### `json` / `dson` — serialize
`parse(text)`→Doge value · `emit(value)`→Str (compact). Maps object↔Dict, array↔List,
number↔Int|Float, null↔none. Unserializable value or bad input → `TypeError`/`ValueError`.
`dson` is the same shape in doge-speak (objects `such … wow`, arrays `so … many`, `yes`/`no`/`empty`, octal numbers).

### `nap` — time (seconds as Float)
`now()`→Float (Unix epoch, UTC) · `mono()`→Float (monotonic; use differences for benchmarking) ·
`rest(seconds)` (sleep) · `stamp(secs)`→ISO-8601 Str · `parse(iso)`→Float.

### `roll` — random
`seed(n)` · `int(low, high)`→Int (inclusive) · `float()`→Float in `[0,1)` · `choice(list)`→element ·
`shuffle(list)`→new List · `sample(list, k)`→new List of k distinct positions.
Seeding is per-pup; `shuffle`/`sample` return new lists (don't mutate).

### `chase` — subprocess
`run(cmd, args, stdin)`→`{code, stdout, stderr}`. `cmd` is a Str, `args` a List of Str,
`stdin` a Str or `none`. Both output streams captured; launch failure is `IOError`.
