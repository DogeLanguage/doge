# Doge Grammar

A sketch of the grammar in EBNF. The parser is hand-written recursive descent
(contextual keywords demand it); this sketch is the reference it follows. For the
prose description of each construct see [SYNTAX.md](SYNTAX.md).

```ebnf
script      = { statement } , "wow" ;
statement   = decl | const | assign | if | for | while
            | func_def | obj_def | pls | return | bonk | amaze | bork | continue
            | import | expr_stmt | bark ;

decl        = "such" , names , "=" , rhs ;
const       = "so" , IDENT , "=" , expr ;
import      = "so" , ( IDENT | STRING ) ;
assign      = [ "very" ] , ( target , augop , expr           (* augmented: one target *)
                           | targets , "=" , rhs ) ;         (* plain / destructuring *)
augop       = ( "+" | "-" | "*" | "/" | "//" | "%" | "**"
              | "&" | "|" | "^" | "<<" | ">>" ) , "=" ;   (* e.g. += //= <<= *)
names       = IDENT , { "," , IDENT } , [ "," , "many" , IDENT ] ;
targets     = target , { "," , target } , [ "," , "many" , target ] ;
rhs         = expr , { "," , expr } ;    (* >=2 exprs build an implicit list, only
                                            opposite >=2 targets — see SYNTAX §4.1 *)
bark        = "bark" , expr ;
bonk        = "bonk" , expr ;
amaze       = "amaze" , expr , [ "," , expr ] ;   (* assert; optional failure message *)
func_def    = "such" , IDENT , [ "much" , params ] , ":" , block , "wow" ;
obj_def     = "many" , IDENT , [ "much" , IDENT ] , ":" , block , "wow" ;
                                                        (* block of func_defs; "much IDENT" names a parent class *)
super_call  = "super" , "." , IDENT , "(" , [ call_args ] , ")" ;   (* positional args only *)
pls         = "pls" , block , "oh no" , IDENT , "!" , block ;
block       = NEWLINE , INDENT , { statement } , DEDENT ;

params      = param , { "," , param } , [ "," , "many" , IDENT ]
            | "many" , IDENT ;
param       = IDENT , [ "=" , literal ] ;     (* required params precede defaulted *)
call        = postfix , "(" , [ call_args ] , ")" ;
call_args   = arg , { "," , arg } ;
arg         = expr | IDENT , "=" , expr ;     (* keyword args follow positional *)
```

A `literal` here is a number, string, bool, `none`, a unary minus on a number, or a
list/dict of literals — see [SYNTAX.md](SYNTAX.md) §6.

## Expression precedence

Loosest to tightest, the recursive-descent parser layers expressions as:

```
ternary   = or , [ "if" , or , "else" , ternary ] ;   (* right-associative *)
or        → and → not → comparison
comparison= bitor , [ compare_op , bitor ] ;           (* non-chaining *)
bitor     → bitxor → bitand → shift → add → mul
unary     = ( "-" | "~" ) , unary | power ;
power     = postfix , [ "**" , unary ] ;               (* right-associative *)
postfix   = primary , { call | subscript | attr } ;
primary   = literal | IDENT | super_call | "(" , expr , ")" | list | dict ;
subscript = "[" , ( expr | [ expr ] , ":" , [ expr ] , [ ":" , [ expr ] ] ) , "]" ;
```

`**` binds tighter than a unary minus on its left but its exponent is a full
unary expression (so `2 ** -1` parses). The bitwise levels are `|` < `^` < `&` <
shifts, all between the comparisons and `+`/`-`, matching Python.

## Disambiguation

- `such IDENT =` is a variable declaration
- `such IDENT :` or `such IDENT much …` is a function definition
- `many IDENT :` is an object definition; `many IDENT much IDENT :` inherits, the
  second name being the parent class (see [SYNTAX.md](SYNTAX.md) §8)
- `so IDENT =` is a constant; `so IDENT` followed by a newline is an import;
  `so STRING` is a string-path import
- an import `so IDENT` resolves in order: a built-in module if one matches, then a
  declared dependency of the importing file's project, then the user module
  `IDENT.doge` next to the importing file (a name that is both a dependency and a
  sibling file is an ambiguity error); `so "sub/dir/NAME.doge"` names a user module
  by a `/`-separated path relative to the importing file and binds its stem `NAME`
  (see [SYNTAX.md](SYNTAX.md) §9 and [PACKAGING.md](PACKAGING.md))
- `much` never starts a statement; it introduces a function's parameters after
  `such NAME`, or a parent class after an object's `many NAME`
- `super` only appears as `super.method(…)` inside a method body — never as a bare
  value or a field access
- `many` at statement level begins an object definition; inside a `much` parameter
  list it marks the trailing variadic parameter (`much host, many guests`); inside
  a destructuring target list it marks the trailing collector (`such a, many rest =
  xs`, `for k, many rest in …:`) — in both target uses it must be the last target
- a `such`/`for` header with a comma-separated target list is always a
  destructuring binding (a function or object header has a single name), so `=`
  and values must follow
- inside a call's parentheses, `IDENT =` begins a keyword argument; every keyword
  argument must follow all positional ones
- inside `[ … ]`, a `:` switches a subscript from a plain index to a slice; each
  of the slice's three parts is optional (`xs[:2]`, `xs[::-1]`)

## Lexer notes

- Indentation-aware: emits `INDENT`/`DEDENT` tokens (spaces only; a tab in leading
  whitespace is a compile error).
- Operators lex longest-match first, so `**=` beats `**` beats `*`, and `<<`/`>>`
  beat `<`/`>`. Every arithmetic and bitwise operator has an `op=` augmented form.
- `oh no` is a compound keyword: the lexer fuses adjacent `oh` + `no` tokens.
- Newlines end statements, except inside an unclosed `(`, `[`, or `{`, where lines
  join implicitly.
- A double-quoted string with `{expr}` holes lexes to an interpolated string
  token: each hole is found on the same physical line (matching nested `{ }` and
  skipping nested string literals) and lexed into its own token stream, which the
  parser reads as a full expression. `\{` escapes a literal brace; an empty or
  unclosed hole is a lex error. The recognized escapes are `\n \t \r \0 \" \\ \{
  \} \xNN \u{…}` (`\xNN` is two hex digits `00`–`7f`; `\u{…}` is 1–6 hex naming a
  Unicode scalar); any other `\c` is a lex error.
