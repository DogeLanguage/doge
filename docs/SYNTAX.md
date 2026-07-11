# Doge Syntax

Keywords, literals, and every statement form in the language. For the formal
grammar see [GRAMMAR.md](GRAMMAR.md); for builtins and modules see
[STDLIB.md](STDLIB.md).

## 1. Keywords

### 1.1 Doge keywords

| Keyword | Context | Meaning | Example |
|---|---|---|---|
| `pls` | statement | `try` (opens its block bare, no `:`) | `pls` |
| `oh no` | after `pls` block | `catch` (binds the error; header ends in `!`, not `:`) | `oh no error!` |
| `bonk` | statement | `raise` (raises a catchable error whose message is the value) | `bonk "much fail"` |
| `bork` | inside a loop | `break` | `if done: bork` |
| `bark` | statement | print/output/execute | `bark "much hello"` |
| `wow` | after a definition body | closes a function or object definition | `wow` |
| `wow` | end of file | required script terminator | `wow` |
| `such` | name + `=` | variable declaration (`let`) | `such age = 7` |
| `such` | name + `:` block | function definition (there is no `def`) | `such greet much name, mood:` |
| `much` | after a function name | parameter list introducer | `such greet much name, mood:` |
| `many` | name + `:` block | object/struct definition | `many Shibe:` |
| `many` | last target/param | trailing collector (variadic param, or destructuring rest) | `such head, many rest = xs` |
| `so` | top-level, bare name | import | `so math` |
| `so` | with `=` | constant (immutable binding) | `so PI = 3.14` |
| `very` | statement start | reassignment (flavored alias for plain `x = ...`) | `very age = 8` |

`such` and `so` are contextual keywords: the token(s) following them disambiguate
the meaning (`such name =` is a variable, `such name … :` is a function; `so name =`
is a constant, bare `so name` is an import). This is handled naturally by a
hand-written recursive-descent parser (same technique as `async` in other languages).
`much` has exactly one job: it appears only inside a function header, between the
name and its parameters.

`oh no` is a compound keyword: the lexer fuses adjacent `oh` + `no` tokens.

### 1.2 Universal keywords (kept as-is)

`if`, `elif`, `else`, `for`, `while`, `in`, `return`, `continue`,
`and`, `or`, `not`, `true`, `false`, `none`

`in` has two jobs: it introduces the iterable in a `for` header (`for x in xs:`)
and, everywhere else, it is the membership operator (see §3).

### 1.3 Reserved for future use

`amaze`, plus `def` and `class`, reserved so the compiler can greet Python muscle
memory with a friendly hint (`"no def here. such greet much name: is the way"`).

## 2. General shape

- Indentation-based blocks; a `:` at the end of a line opens a block (Python-style).
  Exception: error handling. `pls` opens its block bare (no `:`), and `oh no name!`
  ends in `!` instead of `:` (it's an exclamation, after all).
- `#` starts a comment to end of line.
- Statements end at newline; no semicolons.
- Function and object definitions must close with `wow` on its own line, aligned
  with the definition it closes. Control-flow blocks (`if`, `for`, `while`, `pls`/
  `oh no`) end by dedent alone, no `wow`.
- Every script must end with `wow` at top level. A missing `wow`, whether after a
  definition or at the end of the script, is a friendly compile error
  (`"very incomplete. such missing wow. (did the script end early?)"`).
- Indent with spaces, never tabs. A tab in leading whitespace is a friendly compile
  error (`"very tab. much confuse."` with the hint `such fix: indent with spaces`).
  Doge picks one way to avoid the space-vs-tab ambiguity that bites Python.
- Comparisons do not chain. `1 < x < 10` is a compile error; write `1 < x and x < 10`
  (`such fix: use and`). May become real chaining later.
- Lines join implicitly inside brackets. A newline inside an unclosed `(`, `[`, or `{`
  does not end the statement, so list and dict literals may span multiple lines.

## 3. Literals and types

Dynamic value types (all runtime-checked):

| Type | Literal examples |
|---|---|
| Int | `42`, `-7` (i64) |
| Float | `3.14` (f64) |
| Str | `"much wow"` (double quotes, `\n` escapes, `{expr}` interpolation) |
| Bool | `true`, `false` |
| None | `none` |
| List | `["kabosu", "cheems"]` |
| Dict | `{"name": "kabosu", "age": 18}` |
| Function | `such name much params:` definitions |
| Object | instances of `many Name:` definitions |
| Error | the value `oh no err!` binds; `err.type` / `err.message` / `err.file` / `err.line` (see §7) |

Operators: `+ - * / // % ** == != < <= > >= in and or not`, the bitwise
operators `& | ^ ~ << >>`, membership `x in xs` and `x not in xs`, indexing
`xs[0]`, slicing `xs[1:3]`, string concatenation with `+`. Truthiness follows
Python (empty string/list/dict, `0`, `none`, `false` are falsy).

`**` is exponentiation: `2 ** 10` is `1024`. It is right-associative
(`2 ** 3 ** 2` is `2 ** 9`) and binds tighter than a unary minus on its left, so
`-2 ** 2` is `-(2 ** 2)` — but its exponent may be a unary expression, so
`2 ** -1` is `0.5`. `Int ** <non-negative Int>` stays an Int (and overflow is a
catchable error); a negative exponent or any Float operand yields a Float, and
`0 ** <negative>` is a catchable division by zero.

The bitwise operators `& | ^ << >>` and unary `~` work on Ints only (anything
else is a catchable type error). Their precedence follows Python: loosest to
tightest `|`, `^`, `&`, then the shifts `<< >>`, all sitting between the
comparisons and `+`/`-`. `>>` is an arithmetic (sign-preserving) shift; a `<<`
that would drop significant bits, or any shift by a negative or ≥64 count, is a
catchable error rather than a silent wraparound.

**Negative indexing and slicing.** An index may be negative, counting from the
end: `xs[-1]` is the last element and `"kabosu"[-1]` is `"u"`. A slice
`xs[start:end:step]` returns a new List (or, for a Str, a new Str), with every
part optional: `xs[1:3]`, `xs[:2]`, `xs[3:]`, `xs[:]`, `xs[::2]`, and the
reversing `xs[::-1]` all work. Bounds count from the end when negative and clamp
when out of range (never an error); a `step` of `0` is a catchable error, and a
negative `step` walks backward. Slicing is character-based on a Str, matching
indexing.

**Ternary (conditional expression).** `a if cond else b` evaluates to `a` when
`cond` is truthy and `b` otherwise, and only the taken branch runs:
`such mood = "excite" if treats > 3 else "sad"`. The `else` branch is required,
and a chain nests to the right (`a if p else b if q else c` is
`a if p else (b if q else c)`).

**Augmented assignment.** `target op= value` reads the target, applies a binary
operator, and stores the result back — available for every arithmetic and
bitwise operator (`+= -= *= /= //= %= **= &= |= ^= <<= >>=`) and on any
assignable target (a name, an item like `xs[0]`, or a field like `dog.age`). The
target's base and index are evaluated once, so `xs[next()] += 1` calls `next()`
a single time. It obeys the same rules as a plain assignment: the name must be
declared, and a `so` constant cannot be reassigned. `very` may precede it
(`very count += 1`).

Lists and dicts carry methods called on the value — `xs.append(1)`, `d.keys()` —
with no import (see [STDLIB.md](STDLIB.md)). Dicts are insertion-ordered: iterating
their keys/values and printing a dict follow the order keys were first inserted.
A `for` loop over a dict (`for k in d`) walks its keys in that same order.

String interpolation: any double-quoted string may embed expressions in `{…}`
holes, evaluated and spliced in left to right:

```doge
such name = "kabosu"
such age = 7
bark "much hello {name}, age {age + 11}"     # much hello kabosu, age 18
```

A hole holds any single expression — arithmetic, a call, an index, a field, even
a nested string (`"{strings.beeg(name)}"`). Each hole's value is rendered with its
display form, the same text `bark` prints and `str(x)` returns, so numbers,
`none`, lists, and objects interpolate without an explicit `str(…)`. Braces are
always active: write `\{` for a literal `{` (a bare `}` outside a hole is already
literal), so a dict-looking string is `"\{\"a\": 1}"`. An empty hole `{}` and a
hole that never closes are compile errors.

`and` and `or` always evaluate to a Bool and short-circuit: `a or b` skips
`b` when `a` is truthy, `a and b` skips `b` when `a` is falsy. The result is the
truthiness of the deciding operand as a `true`/`false`, never the operand value
itself.

Membership: `x in xs` and its negation `x not in xs` test whether `x` is
contained in `xs`, always yielding a Bool. What "contained" means follows the
right-hand type: a List tests element membership (using `==`, so `1 in [1.0]` is
`true`), a Dict tests whether `x` is one of its keys, and a Str tests whether `x`
is a substring. `in` does not chain, so write `a in b and b in c`, not
`a in b in c`. `not` binds looser than membership, so `not x in xs` means
`not (x in xs)` — the same as `x not in xs`. Any other right-hand type, and a
non-Str left-hand side of a Str test, is a catchable type error (`pls`/`oh no`).

```doge
such pets = ["kabosu", "cheems"]
bark "cheems" in pets                 # true
bark "doge" not in pets               # true
such ages = {"kabosu": 18}
bark "kabosu" in ages                 # true — dict membership tests keys
bark "bos" in "kabosu"                # true — substring
```

Numeric semantics: `/` always
returns a Float, `//` is integer division, Int and Float mix freely with automatic
promotion, and overflow is a catchable runtime error. String indexing and `len()`
count characters, not bytes.

Builtins (always in scope, no import): `len(x)` (character/element count),
`str(x)`, `int(x)`, `float(x)` (conversions), and `range`. `range(n)` yields the
Ints `0 … n-1` as a List; `range(a, b)` yields `a … b-1`; both bounds must be Ints
and the List is empty when the end is not past the start. Details in
[STDLIB.md](STDLIB.md).

## 4. Variables and constants

```doge
such age = 7          # declaration (let)
age = 8               # reassignment
very age = 9          # reassignment, flavored (identical semantics)
age += 1              # augmented reassignment (age = age + 1)
so PI = 3.14          # constant, reassigning is a compile error
```

Declaring with `such` is required before use; assigning to an undeclared name is an
error (catches typos, unlike Python). Augmented assignment (`age += 1`, and every
other `op=` from §3) is reassignment too, so the name must already be declared
and a `so` constant cannot be its target.

### 4.1 Multiple assignment (destructuring)

Both `such` declarations and reassignment accept several targets at once,
unpacking one right-hand value across them:

```doge
such a, b = [1, 2]      # a is 1, b is 2
a, b = b, a             # swap — the right side is read in full first
such head, many rest = [1, 2, 3, 4]   # head is 1, rest is [2, 3, 4]
p, q[0], dog.age = values             # any assignable target: name, item, field
```

- The right-hand value is unpacked with the same rule a `for` loop iterates
  (§5): a List's elements, a Str's characters, or a Dict's keys. Without a
  collector the count must match exactly; a mismatch or a non-iterable value is a
  **catchable runtime error** (`pls`/`oh no`), not a compile error.
- A trailing `many name` collector gathers every surplus value into a List (it
  must be the last target and needs at least one fixed target before it). With a
  collector the value only needs at least as many elements as the fixed targets.
- The whole right side is evaluated before any target is stored, which is what
  makes the swap `a, b = b, a` work. In a reassignment every target is stored
  left to right.
- A comma-separated *right* side builds an implicit List, but only opposite two
  or more targets: `a, b = b, a` works, while `such z = 1, 2` is an error (write
  `such z = [1, 2]`). Augmented assignment (`+=` …) stays single-target, and `so`
  constants are single-name.
- Each destructuring name is a fresh binding like any `such`/`for` variable, so
  the same name may not appear twice in one target list.

## 5. Control flow

```doge
if age > 10:
    bark "such old"
elif age > 5:
    bark "much adult"
else:
    bark "so smol"

for shibe in shibes:
    if shibe == "walter":
        bork              # break
    bark shibe

for name, age in dog.items():   # destructure each element (see §4.1)
    bark "{name} is {age}"

while hungry:
    eat()
```

A `for` loop walks a List's elements, a Str's characters (character-based,
matching the indexing rules above), or a Dict's keys in insertion order (like
`d.keys()`); looping over any other value is a catchable runtime error. The
sequence is a snapshot taken when the loop starts, so mutating the value inside
the body does not change what the loop visits. `while` re-evaluates its condition
before every pass.

A `for` header may name several loop variables (`for k, v in …:`), or a trailing
`many rest` collector, to destructure each element as it is visited — the same
unpacking rule as multiple assignment (§4.1). An element that does not match the
variable count is a catchable runtime error.

## 6. Functions

```doge
such greet much name, mood:
    return "much hello {name}, very {mood}"
wow

such no_args:             # `much` omitted when there are no parameters
    bark "such function"
wow

such invite much host, mood = "excite", many guests:
    bark "{host} is {mood} to see {len(guests)} friends"
wow

invite("kabosu")                          # mood defaults, guests is empty
invite("kabosu", "sleepy", "cheems")      # guests = ["cheems"]
invite("kabosu", mood = "sleepy")         # keyword argument
```

`such` defines both variables and functions: `=` after the name means variable,
`:` (with optional `much` parameters before it) means function. Every function body
closes with `wow`. Calls are conventional: `greet("kabosu", "excite")`, and a
call's argument count must match the definition (checked at compile time for a
direct call, at run time through a value).

Parameters, defaults, keyword arguments, and variadics:

- A parameter may carry a **default**: `much name, mood = "happy"`. A default must
  be a literal — a number, string, bool, `none`, a unary minus on a number, or a
  list/dict of literals — so it names nothing and has no side effects. It is
  evaluated **fresh at every call**, so a `[]` default never leaks between calls.
  Required (no-default) parameters must come before defaulted ones.
- A call may pass **keyword arguments** after its positional ones:
  `greet("kabosu", mood = "excite")`. Positional arguments must come first, each
  keyword name must match a parameter and may appear once, and arguments evaluate
  left to right as written even when keywords reorder them. Keyword arguments work
  where doge knows the function at compile time — a top-level function, a
  constructor (`Shibe(name = "doge")`), or an imported module function
  (`utils.pad(s, width = 4)`). On a method call, a call through a stored function
  value, or a builtin, pass arguments positionally.
- A trailing `many rest` parameter is **variadic**: it gathers every surplus
  positional argument into a List (empty when there are none). It comes last, takes
  no default, and cannot be filled by keyword. Defaults and variadics apply to every
  call form — direct, method, or through a function value.
- A call that supplies too few or too many arguments is an arity error, worded as a
  range: `greet takes 1 to 2 arguments, got 0`, or `party takes at least 1
  argument, got 0` for a variadic function.

Scope and calling rules:

- Functions nest. A `such name:` may be defined inside another function; it is
  local to the enclosing body, just like a `such` variable.
- A function name is unique within its scope. It may not repeat, shadow another
  name in the same scope (a parameter, variable, or sibling function), or take a
  builtin's name. A function name is a fixed binding — reassigning it is an error.
- Functions may read and reassign top-level names. A `such`, `for` variable, or
  caught error introduced inside a function is local to that function; its parameters
  are locals too.
- Closures capture enclosing variables by sharing. A nested function reads and
  writes the enclosing variables it mentions, and the sharing is live in both
  directions: a reassignment inside the closure is visible outside, and a later
  change outside is visible to the closure. Each call of the enclosing function
  makes a fresh set of captured variables, so two closures built on separate calls
  never share (a counter factory hands out independent counters).
- Missing or bare `return` yields `none`. Falling off the end of a body returns
  `none`, and `return` with no value does the same.
- Recursion is depth-limited. A call chain more than 1000 calls deep stops with a
  catchable error rather than exhausting the machine. A closure calling itself
  through its captured name counts the same way.

Functions are values:

- A function name used as a value produces a first-class function you can store,
  pass as an argument, return, and later call: `such g = greet` then `g("kabosu")`.
  Builtins (`such f = len`) and module functions (`such s = nerd.sqrt`) become
  values the same way. Object definitions are not values yet (`such c = Shibe` is a
  compile error) — that lands in a later milestone. Collection methods are not
  first-class either: `such f = xs.append` is a catchable runtime error, since
  `xs.append` reads a field before any call.
- Calling by name is checked at compile time: the argument count must match the
  definition. Calling through a variable or expression is checked at run time — a
  wrong count, or calling something that is not a function, is a catchable error
  (`pls`/`oh no`).
- `bark`ing a function prints `<function name>`. Two function values are equal only
  when they come from the same definition over the same captured variables, so
  `greet == greet` is `true` while two counters from a factory are not. A function
  is always truthy.

## 7. Error handling

```doge
pls
    such result = risky_thing()
oh no err!
    bark "very error: " + err
```

- `pls` opens the try block bare, with no `:`. `oh no <name>!` binds the error and
  opens the handler; the header ends with `!` instead of `:`.
- Errors are structured values. `oh no err!` binds `err` to an `Error` carrying
  four fields:

  | Field | Type | Meaning |
  | --- | --- | --- |
  | `err.type` | `Str` | the category, one of `TypeError`, `DivisionByZero`, `Overflow`, `IndexOutOfBounds`, `KeyError`, `ValueError`, `AttrError`, `Bonk`, `RecursionLimit` |
  | `err.message` | `Str` | the plain-English message |
  | `err.file` | `Str` | the script the error was raised in |
  | `err.line` | `Int` | the line it was raised at |

  An `Error` displays (and interpolates, and `str()`s) as its `message`, and
  concatenates with a `Str` as that message, so `bark "caught: " + err` and
  `"caught: {err}"` both read the message. Any field other than the four above is
  a catchable `AttrError`. Two `Error`s are equal when their type, message, and
  raise site all match. An `Error` is always truthy; indexing, looping, or
  ordering one is a catchable `TypeError`. An `Error` has no methods, so calling
  one is a catchable `AttrError` (`an Error has no methods`) — the same message
  every method-less type gives.
- `bonk <expr>` raises an error of your own. For an ordinary value the type is
  `Bonk` and the message is `<expr>`'s display form — the text `bark` would print
  — so `bonk "much fail"` caught by `oh no err!` gives `err.message == "much fail"`.
  Re-raising a caught error with `bonk err` preserves its original type, message,
  and raise location, so you can handle some errors and re-raise the rest:
  `if err.type == "KeyError": … else: bonk err`.
- Runtime errors (division by zero, missing key, wrong types for an operator) and
  `bonk`s are catchable with `pls`/`oh no`; an uncaught error exits with a
  doge-flavored message and the source line it came from (see [ERRORS.md](ERRORS.md)).

## 8. Objects

```doge
many Shibe:
    such init much name, age:
        self.name = name
        self.age = age
    wow

    such speak:
        bark self.name + " says bork"
    wow
wow

such kabosu = Shibe("kabosu", 18)
kabosu.speak()
```

Each method closes with `wow` at its own indentation level; the final `wow` closes
the object definition.

Single-level object model: fields + methods, `self`, `init` constructor. No
inheritance in v1. The rules:

- Objects are defined at the top level, and a class name is unique like any
  other top-level name. A `many` nested inside a function or block is a compile
  error. Method names are unique within their object.
- `Shibe(...)` builds an instance. The argument count is checked at compile
  time against `init`'s parameters; a class without `init` takes no arguments.
  `init` runs on the new object and its return value is ignored, so `Shibe(...)`
  always evaluates to the object. Otherwise `init` is an ordinary method.
- Fields appear on assignment. `self.name = x` (or `kabosu.name = x` from
  outside) creates the field if it is new. Reading a field that was never set is a
  catchable error; so is reading or setting a field on a non-object.
- Methods dispatch on the receiver's object at run time, and their argument
  count is checked there too: a wrong count or an unknown method is a catchable
  error. Calling a method counts against the 1000-call recursion limit, exactly
  like a function.
- `self` names the receiver inside a method; it is a local, not a parameter
  you declare.
- Objects compare by identity (two instances are equal only when they are the
  same object) and are always truthy. `bark`ing one prints `<Shibe>` (the class
  name in angle brackets).

## 9. Imports

```doge
so nerd
so strings

bark nerd.sqrt(16)
```

A `so <name>` import lives at the top of the script (an import nested in a
function or block is a compile error) and binds the module name for the whole
script. A module is used by member, like `nerd.sqrt(16)` or `strings.beeg("wow")`,
and a member is either a function or a constant (`nerd.pi`). Using the bare module
name as a value, or calling it directly, is a compile error, as is naming an
unknown module or an unknown member.

The available built-in modules (`nerd`, `strings`) are documented in
[STDLIB.md](STDLIB.md). There is no `math` module; the math module is `nerd`.
List and dict operations are methods on the value (`xs.append(1)`), not a module.

### Importing other `.doge` files

`so <name>` first checks the built-in modules; if none matches, it loads the
user module `<name>.doge` from the same directory as the importing file. The
same member syntax applies — `utils.square(6)`, `utils.ANSWER` — and a module
function can be taken as a first-class value (`such f = utils.square`).

```doge
# utils.doge — a module defines things only
so ANSWER = 42

such square much x:
    return x * x
wow
wow

# main.doge
so utils

bark utils.square(6)   # 36
bark utils.ANSWER      # 42
wow
```

A module file may contain **only** definitions — functions, constants (`so X =`),
and its own imports. A loose top-level statement (a `bark`, a loop, a bare
expression) is a compile error: importing a module never runs its code, it only
makes its definitions available. A module's constants are evaluated once, at
program start, in dependency order (a module before anything that imports it).

A module may import other user modules (and the built-in modules). A circular
import — two files that import each other — is a compile error naming the cycle.
A user file whose name collides with a built-in module (`nerd.doge`) is a compile
error, since the built-in always wins. Objects (`many`) in a module are not yet
importable and land in a later milestone.

## 10. Complete example

```doge
so nerd

so GREETING = "much hello"

such greet much name:
    bark GREETING + " " + name
wow

such shibes = ["kabosu", "cheems", "walter"]

for shibe in shibes:
    pls
        greet(shibe)
    oh no err!
        bark "very error: " + err

wow
```
