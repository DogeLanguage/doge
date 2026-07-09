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
| Str | `"much wow"` (double quotes, `\n` escapes) |
| Bool | `true`, `false` |
| None | `none` |
| List | `["kabosu", "cheems"]` |
| Dict | `{"name": "kabosu", "age": 18}` |
| Function | `such name much params:` definitions |
| Object | instances of `many Name:` definitions |

Operators: `+ - * / // % == != < <= > >= and or not`, indexing `xs[0]`, string
concatenation with `+`. Truthiness follows Python (empty string/list/dict, `0`,
`none`, `false` are falsy).

`and` and `or` always evaluate to a Bool and short-circuit: `a or b` skips
`b` when `a` is truthy, `a and b` skips `b` when `a` is falsy. The result is the
truthiness of the deciding operand as a `true`/`false`, never the operand value
itself.

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
so PI = 3.14          # constant, reassigning is a compile error
```

Declaring with `such` is required before use; assigning to an undeclared name is an
error (catches typos, unlike Python).

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

while hungry:
    eat()
```

A `for` loop walks a List's elements or a Str's characters (character-based,
matching the indexing rules above); looping over any other value is a catchable
runtime error. The sequence is a snapshot taken when the loop starts, so mutating
the list inside the body does not change what the loop visits. `while` re-evaluates
its condition before every pass.

## 6. Functions

```doge
such greet much name, mood:
    return "much hello " + name + ", very " + mood
wow

such no_args:             # `much` omitted when there are no parameters
    bark "such function"
wow
```

`such` defines both variables and functions: `=` after the name means variable,
`:` (with optional `much` parameters before it) means function. Every function body
closes with `wow`. Calls are conventional: `greet("kabosu", "excite")`, and a
call's argument count must match the definition (checked at compile time).

Scope and calling rules:

- Definitions live at the top level. A function is defined once at the top of a
  script; defining a function inside another function lands in a later milestone.
- A top-level function name is unique. It may not repeat, shadow another
  top-level name, or take a builtin's name.
- Functions may read and reassign top-level names. A `such`, `for` variable, or
  caught error introduced inside a function is local to that function; its parameters
  are locals too.
- Missing or bare `return` yields `none`. Falling off the end of a body returns
  `none`, and `return` with no value does the same.
- Recursion is depth-limited. A call chain more than 1000 calls deep stops with a
  catchable error rather than exhausting the machine.
- Calls are by name. Using a function's name as a plain value, or calling through
  a variable or expression, lands in a later milestone.

## 7. Error handling

```doge
pls
    such result = risky_thing()
oh no err!
    bark "very error: " + err
```

- `pls` opens the try block bare, with no `:`. `oh no <name>!` binds the error and
  opens the handler; the header ends with `!` instead of `:`.
- Errors are values: `oh no err!` binds `err` to a `Str` carrying the error's message
  (richer error objects land later).
- `bonk <expr>` raises an error of your own. Its message is `<expr>`'s display form,
  the same text `bark` would print, so `bonk "much fail"` caught by `oh no err!`
  binds `err` to `"much fail"`.
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
so lists

bark nerd.sqrt(16)
```

A `so <name>` import lives at the top of the script (an import nested in a
function or block is a compile error) and binds the module name for the whole
script. A module is used by member, like `nerd.sqrt(16)` or `strings.beeg("wow")`,
and a member is either a function or a constant (`nerd.pi`). Using the bare module
name as a value, or calling it directly, is a compile error, as is naming an
unknown module or an unknown member.

The available modules (`nerd`, `strings`, `lists`) are documented in
[STDLIB.md](STDLIB.md). There is no `math` module; the math module is `nerd`.
Importing other `.doge` files is a later milestone.

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
