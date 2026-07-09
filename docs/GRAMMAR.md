# Doge Grammar

A sketch of the grammar in EBNF. The parser is hand-written recursive descent
(contextual keywords demand it); this sketch is the reference it follows. For the
prose description of each construct see [SYNTAX.md](SYNTAX.md).

```ebnf
script      = { statement } , "wow" ;
statement   = decl | const | assign | if | for | while
            | func_def | obj_def | pls | return | bonk | bork | continue
            | import | expr_stmt | bark ;

decl        = "such" , IDENT , "=" , expr ;
const       = "so" , IDENT , "=" , expr ;
import      = "so" , IDENT ;
assign      = [ "very" ] , target , "=" , expr ;
bark        = "bark" , expr ;
bonk        = "bonk" , expr ;
func_def    = "such" , IDENT , [ "much" , params ] , ":" , block , "wow" ;
obj_def     = "many" , IDENT , ":" , block , "wow" ;    (* block of func_defs *)
pls         = "pls" , block , "oh no" , IDENT , "!" , block ;
block       = NEWLINE , INDENT , { statement } , DEDENT ;
```

## Disambiguation

- `such IDENT =` is a variable declaration
- `such IDENT :` or `such IDENT much …` is a function definition
- `many IDENT :` is an object definition
- `so IDENT =` is a constant; `so IDENT` followed by a newline is an import
- `much` never starts a statement; it only appears inside a function header

## Lexer notes

- Indentation-aware: emits `INDENT`/`DEDENT` tokens (spaces only; a tab in leading
  whitespace is a compile error).
- `oh no` is a compound keyword: the lexer fuses adjacent `oh` + `no` tokens.
- Newlines end statements, except inside an unclosed `(`, `[`, or `{`, where lines
  join implicitly.
