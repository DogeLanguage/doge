# Doge Standard Library

Builtins are always in scope; modules are pulled in with `so <name>` at the top of
the script (import syntax and rules: [SYNTAX.md](SYNTAX.md) §9). All of it is
implemented in Rust inside `doge-runtime`.

## Builtins (no import needed)

| Builtin | Meaning |
|---|---|
| `len(x)` | character count of a Str, byte count of a Bytes, element count of a List/Dict |
| `str(x)` | convert to Str (the same display form `bark` prints) |
| `int(x)` | convert to Int: a whole number of any size from a Str, a Float/Decimal truncated toward zero, a Bool to 0/1 |
| `float(x)` | convert to Float |
| `dec(x)` | convert to an exact Decimal: a Str/Int exactly, a Float via its shortest decimal form (so `dec(0.1)` is `0.1`, not binary noise), a Decimal unchanged |
| `bytes(x)` | convert to Bytes: a Str is UTF-8-encoded, a List of Ints (each 0–255) becomes those bytes, a Bytes is returned unchanged |
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

### Bytes

`Bytes` is raw binary data — the byte-based counterpart of the char-based `Str`,
for data that is not text (file contents, hashes, binary formats). Build one with
`bytes(x)`; there is no literal. It is immutable, like `Str`.

Where `Str` operations count characters, `Bytes` operations count bytes: `len(b)`
is the byte count and `b[i]` is an `Int` 0–255. Iterating a `Bytes` yields those
Ints, `int in b` tests byte membership (and `bytes in b` a contiguous sub-slice),
`b + b` concatenates, slicing yields a `Bytes`, and `==`/ordering compare
byte-wise. Printing (and `str(b)`) shows the `b"..."` form — printable ASCII
literally, other bytes as `\xNN`.

| Method | Returns | Meaning |
|---|---|---|
| `hex()` | `Str` | the bytes as lowercase hexadecimal (`bytes("hi").hex()` is `"6869"`) |
| `decode()` | `Str` | the bytes decoded as UTF-8 text; invalid UTF-8 is a catchable `ValueError` |

```doge
such raw = bytes("hi")     # b"hi"
bark len(raw)              # 2
bark raw.hex()             # 6869
bark raw.decode()          # hi
```

### Decimal

`Decimal` is exact base-10 arithmetic — the companion to `Float` for money and any
calculation that must be exact rather than the nearest binary approximation. Build
one with `dec(x)`; there is no literal. It has no methods (operators do the work).

A `Decimal` is exact and arbitrary precision. It participates in the ordinary
operators, with two rules that keep exactness honest:

- **Int and Decimal mix** (both are exact), promoting to `Decimal`: `1 + dec("0.5")`
  is `dec("1.5")`.
- **Float and Decimal do not mix** in arithmetic — silently folding an inexact
  `Float` into an exact `Decimal` would corrupt it, so it is a catchable `TypeError`.
  Convert one side first (`dec(x)` or `float(x)`). Comparison still works across all
  three numeric types.

`Decimal / Decimal` is a `Decimal` (exact when the division terminates, otherwise
rounded to a fixed scale). `//` and `%` follow the same floor/modulo rules as Ints,
and `Decimal ** <non-negative Int>` stays an exact Decimal. Printing keeps the value's
own scale (`dec("0.10")` shows `0.10`), while equality is by value (`dec("0.10") ==
dec("0.1")`). `int(d)` truncates toward zero; `float(d)` rounds to the nearest Float.

```doge
bark dec("0.1") + dec("0.2")   # 0.3      (Float 0.1 + 0.2 is 0.30000000000000004)
such price = dec("19.99")
bark price * 3                 # 59.97
bark dec("1") / dec("4")       # 0.25
bark dec("0.10") == dec("0.1") # true
```

`dec` and `json`/`dson`: `json.emit` writes a Decimal as a bare JSON number (JSON has
no decimal type, so a round-trip through `json.parse` returns a Float). `dson` numbers
are octal and so have no faithful Decimal form — `dson.emit` of a Decimal is a
catchable `TypeError`, like any other unserializable value.

## Modules

| Module | Members |
|---|---|
| `nerd` | `abs`, `sqrt`, `floor`, `ceil`, `round`, `min`, `max`, `pow`; constants `pi`, `e` |
| `strings` | `beeg` (uppercase), `smoll` (lowercase), `trim`, `split`, `join`, `contains`, `replace` |
| `hunt` | `test`, `find`, `find_all`, `groups`, `replace` — regular-expression matching |
| `fetch` | `read`, `write`, `append`, `read_bytes`, `write_bytes`, `exists`, `delete`, `list`, `make_dir`, `remove_dir`, `rename`, `copy`, `stat`, `join`, `basename`, `ext` — files, directories, metadata, and path helpers |
| `env` | `args`, `get` — command-line arguments and environment variables |
| `howl` | `listen`, `connect`, `accept`, `port`, `send`, `send_bytes`, `recv`, `recv_bytes`, `recv_line`, `close`, `get`, `post` — TCP sockets and an HTTP(S) client |
| `pack` | `zoom`, `fetch`, `bowl`, `drop`, `sniff` — threads (pups) and channels (bowls) |
| `json` | `parse`, `emit` — JSON to and from Doge values |
| `dson` | `parse`, `emit` — DSON (Doge Serialized Object Notation) to and from Doge values |
| `nap` | `now`, `mono`, `rest`, `stamp`, `parse` — clocks, sleep, and UTC timestamps |
| `roll` | `seed`, `int`, `float`, `choice`, `shuffle`, `sample` — random numbers and sampling |
| `chase` | `run` — run an external program and capture its output |
| `crypto` | `sha256`, `hmac_sha256`, `token`, `same` — hashing, HMAC, secure random bytes, and constant-time compare |

A member is either a function, like `nerd.sqrt(16)` or `strings.beeg("wow")`, or a
constant (`nerd.pi`). Arity and unknown-member errors are caught at compile time
from a module table in the compiler that mirrors the runtime.

### `hunt` — regular expressions

Pattern matching over a Str: the dog hunts a pattern through the text. Every member
takes the pattern as its first argument and the text to search as its second; both
must be a Str, or it is a catchable `TypeError`. An invalid pattern is a catchable
`ValueError` (`err.type == "ValueError"`), never a crash. Matches come back as the
matching *substrings*, so character indexing is never a concern.

| Member | Returns | Meaning |
|---|---|---|
| `test(pat, text)` | `Bool` | whether `pat` matches anywhere in `text` |
| `find(pat, text)` | `Str` or `none` | the first substring that matches, or `none` when there is no match |
| `find_all(pat, text)` | `List` of `Str` | every non-overlapping match, in order (an empty List when none) |
| `groups(pat, text)` | `List` or `none` | the capture groups of the first match — group 0 (the whole match) first; a group that did not participate is `none`. `none` overall when `pat` does not match |
| `replace(pat, text, repl)` | `Str` | every match replaced by `repl`, which may reference groups as `$1` or `${name}` |

The pattern syntax is the standard one (character classes `[0-9]`, anchors `^`/`$`,
quantifiers `*`/`+`/`?`, groups `(...)`, alternation `|`); the engine matches in
linear time, so no pattern can hang the program. A backslash escape like `\d` or
`\w` must be written `\\d` / `\\w` in a Doge string literal, since a bare backslash
is a string escape (only `\n`, `\t`, `\"`, `\\`, `\{`, `\}` are known) — the
character classes `[0-9]`, `[a-z]` and the like need no backslash and read fine.

```doge
so hunt

bark hunt.test("^woof", "woof woof")           # true
bark hunt.find("[0-9]+", "order 42 now")       # 42
bark hunt.find_all("[0-9]+", "1 22 333")       # ["1", "22", "333"]
bark hunt.groups("(\\w+)@(\\w+)", "doge@shibe") # ["doge@shibe", "doge", "shibe"]
bark hunt.replace("[0-9]+", "a1b22c", "#")     # a#b#c

# A no-match lookup is `none`, and a bad pattern is catchable.
bark hunt.find("nope", "text")                 # none
pls
    hunt.find("[", "text")
oh no err!
    bark err.type                              # ValueError
wow
```

### `fetch` — file I/O

Every path must be a Str, the text operations take/return a Str, and the binary
operations take/return `Bytes`; anything else is a catchable `TypeError`. Every OS
failure — a missing file, a permission problem — is a catchable `IOError`
(`err.type == "IOError"`), never a crash. Read a file whose bytes are not valid
text with `read` and it is an `IOError`; use `read_bytes` for binary files.

| Member | Returns | Meaning |
|---|---|---|
| `read(path)` | `Str` | the whole file as text (missing file or non-text bytes are an `IOError`) |
| `write(path, text)` | `none` | replace the file's contents with `text`, creating it if needed |
| `append(path, text)` | `none` | add `text` to the end of the file, creating it if needed |
| `read_bytes(path)` | `Bytes` | the whole file as raw bytes, for binary data (a missing file is an `IOError`) |
| `write_bytes(path, bytes)` | `none` | replace the file's contents with raw `bytes`, creating it if needed |
| `exists(path)` | `Bool` | whether anything exists at `path` |
| `delete(path)` | `none` | remove the file (a missing file is an `IOError`) |
| `list(path)` | `List` of `Str` | the entry names in a directory, sorted (a missing path or non-directory is an `IOError`) |
| `make_dir(path)` | `none` | create the directory and any missing parents; already existing is not an error |
| `remove_dir(path)` | `none` | remove the directory and everything inside it (a missing path is an `IOError`) |
| `rename(from, to)` | `none` | move or rename a file or directory, replacing `to` if it exists |
| `copy(from, to)` | `none` | copy a file's contents to `to`, creating or replacing it |
| `stat(path)` | `Dict` | metadata about `path` (a missing path is an `IOError`) |
| `join(a, b)` | `Str` | join two path segments with `/` (an absolute `b` replaces `a`) |
| `basename(path)` | `Str` | the final component of a path (`"a/b/c.txt"` → `"c.txt"`) |
| `ext(path)` | `Str` | the extension including the leading dot (`"c.txt"` → `".txt"`), or `""` when none |

`stat` returns a Dict with three keys: `size` (an `Int` byte count), `modified`
(a `Float` of Unix seconds, negative before the epoch), and `is_dir` (a `Bool`).
`join`, `basename`, and `ext` never touch the filesystem — they are pure string
operations on the path, so they only ever raise a `TypeError`, never an `IOError`.

```doge
so fetch
fetch.write("notes.txt", "much wow")
fetch.append("notes.txt", "\nsuch file")
bark fetch.read("notes.txt")

fetch.make_dir("logs")
bark fetch.stat("notes.txt")["size"]
bark fetch.join("logs", "run.txt")
fetch.remove_dir("logs")
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

### `howl` — TCP sockets and HTTP

Networking. Raw TCP for building servers and clients, plus a minimal HTTP(S)
client. Every network failure — a refused connection, an unknown host, a broken
pipe, a TLS or timeout error, or any operation on a closed socket — is a catchable
`IOError` (`err.type == "IOError"`), never a crash. A wrong argument type (a
non-Str host, a non-Socket where one belongs) is a catchable `TypeError`, and a
port outside `0…65535` or a non-positive `recv` size is a catchable `ValueError`.

A **Socket** is a value like any other: it can be stored, passed to a function, or
returned. Sockets are opaque — they have no fields or methods, compare equal only
to the very same socket, and close automatically when the last reference is
dropped. `howl.listen` and `howl.accept` are for servers; `howl.connect` is for
clients; both ends read and write with `send`/`recv`/`recv_line`.

| Member | Returns | Meaning |
|---|---|---|
| `listen(host, port)` | `Socket` | bind a TCP listener on `host:port`; port `0` lets the OS choose a free one |
| `connect(host, port)` | `Socket` | open a TCP connection to `host:port` |
| `accept(listener)` | `Socket` | block until a client connects, then give back the new connection |
| `port(sock)` | `Int` | the local port a listener or connection is bound to (read a port-`0` listener's real port back) |
| `send(conn, text)` | `none` | write `text` to a connection as UTF-8 |
| `send_bytes(conn, bytes)` | `none` | write raw `bytes` to a connection unchanged |
| `recv(conn, max_bytes)` | `Str` or `none` | read up to `max_bytes` bytes as text, or `none` at end of input |
| `recv_bytes(conn, max_bytes)` | `Bytes` or `none` | read up to `max_bytes` raw bytes, or `none` at end of input |
| `recv_line(conn)` | `Str` or `none` | read one line, without the trailing newline (`\r\n` trimmed too), or `none` at end of input |
| `close(sock)` | `none` | close a listener or connection now (idempotent) |
| `get(url)` | `Dict` | HTTP(S) GET → `{"status": Int, "body": Str}` |
| `post(url, body)` | `Dict` | HTTP(S) POST of `body` as `text/plain` → `{"status": Int, "body": Str}` |

`recv` carries one `Str` type: it never splits a multi-byte character across two
reads (an incomplete trailing sequence is held for the next call, so every read
returns at least one whole character or `none`), and bytes that are not valid text
are an `IOError`. `send_bytes`/`recv_bytes` are the binary counterpart: they carry
raw `Bytes` with no UTF-8 validation, so non-text data is never an error and each
`recv_bytes` returns bytes exactly as they arrive — the way to handle a binary
upload or byte-accurate framing (a `Content-Length` body), where `recv`'s
partial-character buffering would get the count wrong. Raw TCP calls block with no timeout; `howl.get`/`howl.post` time
out after 30 seconds (a catchable `IOError`). For HTTP, only a transport, TLS, or
timeout failure is an error — a non-2xx response (a `404`, say) comes back as an
ordinary `{"status", "body"}` Dict, so the script decides what a status means.

```doge
so howl

# A tiny echo server and its client in one script — on loopback, connect() lands
# in the backlog immediately, so one script can drive both ends.
such server = howl.listen("127.0.0.1", 0)
such client = howl.connect("127.0.0.1", howl.port(server))
howl.send(client, "much ping\n")

such conn = howl.accept(server)
bark howl.recv_line(conn)          # much ping
howl.send(conn, "wow pong\n")
bark howl.recv_line(client)        # wow pong

# An HTTP GET returns a status and body Dict.
such page = howl.get("https://example.com")
bark page["status"]                # 200
```

### `pack` — threads and channels

Parallel execution. `pack.zoom` runs a function on its own OS thread — a **pup** —
so real work happens on several cores at once; `pack.fetch` waits for a pup's
result; and a **bowl** is a channel (`bowl`/`drop`/`sniff`) pups pass values over.
Every misuse — a non-callable `zoom`, a second `fetch` of the same pup, a wrong
handle type, an un-sendable value — is a catchable error, never a crash.

| Member | Returns | Meaning |
|---|---|---|
| `zoom(f, args)` | `Pup` | run `f` on a new pup, called with the List `args`; returns a handle to it |
| `fetch(pup)` | the result | block until the pup finishes and return what `f` returned, or re-raise the error it hit |
| `bowl()` | `Bowl` | open a fresh, empty channel |
| `drop(bowl, value)` | `none` | send `value` into a bowl |
| `sniff(bowl)` | the value | block until a value arrives in the bowl, then return it |

**Each pup is its own world.** Doge values are reference-counted and
single-threaded, so a pup never shares mutable state with the thread that spawned
it. Instead, everything a pup needs is **deep-copied** across the boundary when it
starts: the callee and its captured variables, each argument, and a snapshot of the
script's top-level variables as they were at `zoom` time. The return value is copied
back the same way. A pup mutating a copied list or object changes only its own copy
— there are no shared-memory races to reason about, and no locks to hold.

Two things are shared rather than copied, because sharing is their point:

- A **bowl** hands both sides a handle to the *same* channel, so a value dropped in
  one pup can be sniffed in another. `sniff` is first-in first-out and blocks until
  a value is available. Any pup may drop or sniff.
- A **socket** ([`howl`](#howl--tcp-sockets-and-http)) is *transferred* when you
  send it explicitly — as a `zoom` argument or a `drop` payload — the pup takes over
  the live connection and the sender's handle becomes closed. (A socket merely
  caught in the globals snapshot or a closure arrives closed instead, so spawning a
  pup never silently steals a listener you are still using.)

A **Pup** and a **Bowl** are opaque values like a Socket: no fields or methods, and
equal only to the very same handle. A pup itself cannot be sent to another pup (a
catchable `TypeError`), and a self-referential value cannot be copied across (a
catchable `ValueError`). Fetching a pup twice is a catchable error — its result can
only be claimed once. If a pup raises, the error travels back and `fetch` re-raises
it with the pup's own type, message, and source location, so `pls`/`oh no` on the
`fetch` catches it exactly as if the call had run locally.

The script exits when its top-level statements finish; it does **not** wait for
pups still running, so `fetch` whatever results you need before the end. A program
where every thread is blocked in `sniff` with nothing left to drop is a deadlock of
your own making — a bowl whose every writer is gone raises a catchable `IOError`
from `sniff` rather than blocking forever.

```doge
so pack

such square much n:
    return n * n
wow

# Spawn three pups, then fetch in order — fetch blocks, so output is deterministic.
such pups = []
for n in [2, 3, 4]:
    pups.append(pack.zoom(square, [n]))
for p in pups:
    bark pack.fetch(p)             # 4, 9, 16

# A bowl carries values between pups.
such bowl = pack.bowl()
pack.drop(bowl, "treat")
bark pack.sniff(bowl)              # treat
```

### `json` — JSON parse and emit

Structured-data serialization. `json.parse(text)` turns a JSON document into a
Doge value; `json.emit(value)` turns a Doge value back into compact JSON text.
The mapping between the two is direct:

| JSON | Doge |
|---|---|
| object | `Dict` (insertion-ordered; a repeated key keeps its last value) |
| array | `List` |
| string | `Str` |
| number | `Int` when it is written with no fraction or exponent and fits an `Int`, otherwise `Float` |
| `true` / `false` | `Bool` |
| `null` | `none` |

`json.emit` produces the compact form — no spaces, keys in the dict's insertion
order (`{"name":"kabosu","tags":["doge","shibe"]}`). A whole-number `Float` keeps
its point (`3.0`, not `3`) so it re-parses as a `Float`. Only
`Dict`/`List`/`Str`/`Int`/`Float`/`Bool`/`none` have a JSON form: emitting an
object, function, socket, or any other type is a catchable `TypeError`, and a
non-finite `Float` (JSON has no `NaN`/infinity) is a catchable `ValueError`.

Every malformed input is a catchable `ValueError` that names the offset it failed
at — a truncated document, a bad escape, trailing text after the value, or nesting
deeper than 128 levels (the depth cap that keeps a pathological or self-describing
input from exhausting the stack). Nothing here ever crashes.

```doge
so json

such config = json.parse("[1, 2.5, null, true]")
bark config[1]                       # 2.5

such doc = {"name": "kabosu", "good": true}
bark json.emit(doc)                  # {"name":"kabosu","good":true}

pls
    json.parse("[1, 2,")
oh no err!
    bark err.type                    # ValueError
```

### `dson` — DSON parse and emit

DSON — [Doge Serialized Object Notation](https://dogeon.xyz/) — is JSON's shape in
doge-speak, and the `dson` module mirrors `json` member-for-member (`parse`,
`emit`) and maps to the exact same Doge values. Only the surface syntax differs:

- An object is `such … wow`, each pair written `"key" is value`, pairs separated by
  any of `,` `.` `!` `?` (emit uses `,`). An empty object is `such wow`.
- An array is `so … many`, elements separated by `and` or `also` (emit uses `and`).
  An empty array is `so many`.
- `yes` / `no` / `empty` are `true` / `false` / `none`.
- **Numbers are octal.** `17620` is `8080`, `-12` is `-10`, and a fraction or a
  `very`/`VERY` exponent (meaning × 8ⁿ, so `4very2` is `4 × 8² = 256`) makes it a
  `Float`. `dson.emit` writes Ints as plain octal and Floats as their exact octal
  expansion, always with a point (`0.4` for `0.5`) so a whole Float stays a Float.
- A `\u` string escape takes **six octal digits** (not four hex).

The value mapping, the catchable-error contract, and the 128-level depth cap are
identical to [`json`](#json--json-parse-and-emit); the two codecs are
interchangeable but for how the text reads.

```doge
so dson

such doc = {"name": "kabosu", "age": 7, "tags": ["doge", "shibe"]}
bark dson.emit(doc)
# such "name" is "kabosu", "age" is 7, "tags" is so "doge" and "shibe" many wow

such back = dson.parse(dson.emit(doc))
bark back["age"]                     # 7
bark dson.parse("so yes and no and empty many")   # [true, false, none]
```

### `nap` — time and clocks

Reading the clock, sleeping, and turning timestamps into dates. Time is measured
in seconds throughout, as a `Float` (so it mixes freely with the rest of Doge's
numbers and keeps sub-second precision).

| Member | Returns | Meaning |
|---|---|---|
| `now()` | `Float` | seconds since the Unix epoch, UTC (sub-second) |
| `mono()` | `Float` | seconds from a fixed process origin; monotonic, so only *differences* between readings are meaningful |
| `rest(seconds)` | `none` | sleep for `seconds` (an Int or Float) |
| `stamp(secs)` | `Str` | the ISO-8601 UTC string for a unix timestamp |
| `parse(text)` | `Float` | unix seconds for an ISO-8601 UTC string |

Use `now()` for wall-clock timestamps and `mono()` for benchmarking — `mono()`
never jumps when the system clock is adjusted, so `nap.mono() - start` is a
trustworthy elapsed time. `now()` never fails; a system clock set before the
epoch simply reads back negative.

`stamp` formats to whole-second UTC — `nap.stamp(0)` is `"1970-01-01T00:00:00Z"` —
and `parse` reads that same shape (`"YYYY-MM-DDTHH:MM:SSZ"`, the trailing `Z`
optional) back to seconds, so the two round-trip. A duration that is negative,
non-finite, or absurdly large is a catchable `ValueError` from `rest` rather than
a crash, and a timestamp `parse` cannot read — wrong layout, or a field out of
range — is a catchable `ValueError` too.

```doge
so nap

such start = nap.mono()
nap.rest(0.05)
bark nap.mono() - start >= 0.05      # true

bark nap.stamp(946684800)            # 2000-01-01T00:00:00Z
bark nap.parse("2000-01-01T00:00:00Z") == 946684800   # true

pls
    nap.parse("not a date")
oh no err!
    bark err.type                    # ValueError
```

### `roll` — random numbers and sampling

Rolling dice, picking, shuffling, and drawing samples. The generator is seeded
from the clock on first use, so every run differs; call `seed` to fix the sequence
and make a run reproducible.

| Member | Returns | Meaning |
|---|---|---|
| `seed(n)` | `none` | seed the generator with an Int, so the following draws repeat next run |
| `int(low, high)` | `Int` | a uniform Int in the **inclusive** range `[low, high]` |
| `float()` | `Float` | a uniform Float in `0.0 <= x < 1.0` |
| `choice(list)` | element | one random element of a non-empty List |
| `shuffle(list)` | `List` | a new List with the same elements in random order |
| `sample(list, k)` | `List` | a new List of `k` elements drawn from distinct positions |

`shuffle` and `sample` return a **new** List and leave their argument untouched —
like every module function, they do not mutate in place (that is what list methods
such as `xs.append` are for). `sample` draws by position, so if the List holds
duplicate values the sample may too, but no position is drawn twice.

`int` with `low` above `high` is a catchable `ValueError` (there is no empty
range), `choice` of an empty List is a `ValueError`, and `sample` needs
`0 <= k <= len(list)` or it is a `ValueError` too — never a crash.

Seeding is per **pup**: `seed` fixes only the calling thread's generator, and a pup
spawned by `pack.zoom` starts from its own clock-seeded stream. A single-threaded
script is fully reproducible after `seed`.

```doge
so roll

roll.seed(42)                        # reproducible from here

bark roll.int(1, 6)                  # a d6 roll, 1..6
bark roll.float() < 1.0              # true — always in [0, 1)
bark roll.choice(["heads", "tails"]) # heads or tails
bark roll.shuffle([1, 2, 3])         # e.g. [3, 1, 2]
bark roll.sample([1, 2, 3, 4, 5], 2) # two distinct picks

pls
    roll.choice([])
oh no err!
    bark err.type                    # ValueError
```

### `chase` — subprocess

Run another program. `chase.run` launches a command, waits for it to finish, and
gives back a Dict of what happened. Both output streams are always captured (the
child never writes to your terminal), and failing to launch the program — a missing
binary, a permission problem — is a catchable `IOError` (`err.type == "IOError"`),
never a crash.

| Member | Returns | Meaning |
|---|---|---|
| `run(cmd, args, stdin)` | `Dict` | run `cmd` with the Str `args`, optionally feeding `stdin`, then give back `{"code": Int, "stdout": Str, "stderr": Str}` |

`cmd` is a Str program name or path; `args` is a `List` of `Str`; `stdin` is a Str
to feed the program on its standard input, or `none` to feed it nothing. A wrong
type for any of them — a non-Str `cmd`, an `args` that is not a List of Str, a
`stdin` that is neither a Str nor `none` — is a catchable `TypeError`, checked
before anything is launched.

The result Dict is always `{"code", "stdout", "stderr"}`: `code` is the program's
exit status (`0` on success, `-1` when it was terminated by a signal), and `stdout`
and `stderr` are its captured output as text. Output that is not valid text is an
`IOError`, as with `fetch.read`. Feeding `stdin` to a program that ignores it (or
exits early) is fine — it is not an error — and `chase.run` never hangs, however
much a program reads or writes.

```doge
so chase

such hello = chase.run("echo", ["much", "wow"], none)
bark hello["code"]                   # 0
bark hello["stdout"]                 # much wow

# Feed a program on its standard input.
such shout = chase.run("cat", [], "such input\n")
bark shout["stdout"]                 # such input

pls
    chase.run("doge-no-such-program", [], none)
oh no err!
    bark err.type                    # IOError
```

### `crypto` — hashing, HMAC, and secure randomness

The security primitives an app needs to authenticate a user: hash a password, sign
a session cookie, mint an unguessable token, and compare a secret without leaking it
through timing. Everything speaks `Bytes` — the natural type for a digest or a key —
and a `Bytes` renders for storage or display with its `.hex()` method. `sha256` and
`hmac_sha256` accept a `Str` (hashed as its UTF-8 bytes) or `Bytes` interchangeably.

| Member | Returns | Meaning |
|---|---|---|
| `sha256(data)` | `Bytes` | the 32-byte SHA-256 digest of a `Str` or `Bytes` |
| `hmac_sha256(key, data)` | `Bytes` | the 32-byte HMAC-SHA-256 of `data` under `key`; each is a `Str` or `Bytes` |
| `token(n)` | `Bytes` | `n` cryptographically secure random bytes from the operating system |
| `same(a, b)` | `Bool` | whether two `Str`/`Bytes` are equal, compared in constant time |

`token` draws from the operating system's cryptographically secure random source,
so its bytes are unpredictable and independent of `roll` (which is a fast,
clock-seeded generator meant for dice and shuffles, **not** for secrets). A length
`n` that is zero or negative is a catchable `ValueError`; a wrong type for any
argument is a catchable `TypeError`.

`same` returns a `Bool` like `==`, but its running time does not depend on *where*
two equal-length secrets first differ, so it does not leak a secret one byte at a
time to an attacker measuring how long the comparison took. Use it to check a signed
cookie or an API key; two values of different length are simply unequal.

```doge
so crypto

# A password hash for storage, and an HMAC that signs a session id.
bark crypto.sha256("hunter2").hex()
such cookie = crypto.hmac_sha256("server-key", "user=42")
bark cookie.hex()

# A fresh, unguessable session token.
such session = crypto.token(32)
bark len(session)                    # 32

# Verify a cookie without leaking timing.
bark crypto.same(cookie, crypto.hmac_sha256("server-key", "user=42"))   # true

pls
    crypto.token(0)
oh no err!
    bark err.type                    # ValueError
```

A `so <name>` import that is not a stdlib module resolves to the user file
`<name>.doge` next to the importer; see [SYNTAX.md](SYNTAX.md) §9.
