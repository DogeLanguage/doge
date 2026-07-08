//! Doge AST → Rust source. The generated Rust is thin glue: every
//! value operation calls a function in `doge-runtime`, so behaviour lives there,
//! not in these strings.

use crate::ast::{BinOp, Expr, Script, Stmt, UnOp};
use crate::check::BUILTINS;
use crate::diagnostics::Diagnostic;
use crate::token::Span;

const UNSUPPORTED_HEADLINE: &str = "very soon. much roadmap.";
const ARITY_HEADLINE: &str = "very args. much wrong.";
const RUNTIME_ERROR_HEADLINE: &str = "very error. much broken.";

/// Prefix on every generated identifier — makes Rust-keyword collisions
/// impossible. Never appears in anything the user sees.
const NAME_PREFIX: &str = "v_";

/// Turn a checked [`Script`] into a complete Rust source file, or a diagnostic
/// pointing at the first feature M3 cannot run yet. `path`/`source` are used to
/// render those diagnostics and to embed the script path in the uncaught-error
/// message.
pub fn generate(path: &str, source: &str, script: &Script) -> Result<String, Diagnostic> {
    let codegen = Codegen {
        path: path.to_string(),
        lines: source
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect(),
    };
    codegen.file(script)
}

struct Codegen {
    path: String,
    lines: Vec<String>,
}

/// A language feature the parser accepts but M3 cannot run yet, with the exact
/// message and the milestone that lands it.
enum Unsupported {
    FuncDef,
    Return,
    Call,
    Try,
    ConstDecl,
    Import,
    ObjDef,
    Attr,
}

impl Unsupported {
    fn detail(&self) -> (&'static str, &'static str) {
        match self {
            Unsupported::FuncDef => ("functions don't run yet — they land in M4", "M4"),
            Unsupported::Return => ("return needs functions — they land in M4", "M4"),
            Unsupported::Call => ("calling your own functions lands in M4", "M4"),
            Unsupported::Try => ("pls / oh no doesn't run yet — it lands in M4", "M4"),
            Unsupported::ConstDecl => ("so constants don't run yet — they land in M4", "M4"),
            Unsupported::Import => ("so imports don't run yet — they land in M4", "M4"),
            Unsupported::ObjDef => ("many objects don't run yet — they land in M5", "M5"),
            Unsupported::Attr => ("object attributes land in M5", "M5"),
        }
    }
}

impl Codegen {
    fn file(&self, script: &Script) -> Result<String, Diagnostic> {
        let mut body = String::new();
        for name in hoisted_names(&script.stmts) {
            body.push_str(&format!(
                "    let mut {NAME_PREFIX}{name}: Value = Value::None;\n"
            ));
        }
        for stmt in &script.stmts {
            self.stmt(stmt, 1, &mut body)?;
        }

        let mut out = String::new();
        out.push_str("#![allow(warnings)]\n");
        out.push_str("use doge_runtime::*;\n\n");
        out.push_str("fn main() -> std::process::ExitCode {\n");
        out.push_str("    let mut cur_line: u32 = 0;\n");
        out.push_str("    match run(&mut cur_line) {\n");
        out.push_str("        Ok(()) => std::process::ExitCode::SUCCESS,\n");
        out.push_str("        Err(e) => {\n");
        out.push_str(&format!(
            "            eprintln!(\"{}\\n\\n  {}:{{cur_line}}\\n  {{e}}\");\n",
            RUNTIME_ERROR_HEADLINE,
            escape_str(&self.path)
        ));
        out.push_str("            std::process::ExitCode::FAILURE\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
        out.push_str("fn run(cur_line: &mut u32) -> DogeResult<()> {\n");
        out.push_str(&body);
        out.push_str("    Ok(())\n");
        out.push_str("}\n");
        Ok(out)
    }

    fn stmt(&self, stmt: &Stmt, level: usize, out: &mut String) -> Result<(), Diagnostic> {
        let pad = "    ".repeat(level);
        out.push_str(&format!("{pad}*cur_line = {};\n", stmt_line(stmt)));
        match stmt {
            Stmt::Decl { name, expr, .. } => {
                out.push_str(&format!(
                    "{pad}{NAME_PREFIX}{name} = {};\n",
                    self.expr(expr)?
                ));
            }
            Stmt::Assign { target, expr, .. } => match target {
                Expr::Ident { name, .. } => {
                    out.push_str(&format!(
                        "{pad}{NAME_PREFIX}{name} = {};\n",
                        self.expr(expr)?
                    ));
                }
                Expr::Index { obj, index, .. } => {
                    out.push_str(&format!(
                        "{pad}index_set(&{}, &{}, {})?;\n",
                        self.expr(obj)?,
                        self.expr(index)?,
                        self.expr(expr)?
                    ));
                }
                Expr::Attr { span, .. } => return Err(self.unsupported(*span, Unsupported::Attr)),
                _ => unreachable!("compiler bug: parser guarantees a valid assign target"),
            },
            Stmt::Bark { expr, .. } => {
                out.push_str(&format!("{pad}let _ = bark(&{});\n", self.expr(expr)?));
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (i, (cond, body)) in branches.iter().enumerate() {
                    let head = if i == 0 { "if" } else { "} else if" };
                    out.push_str(&format!("{pad}{head} ({}).truthy() {{\n", self.expr(cond)?));
                    for s in body {
                        self.stmt(s, level + 1, out)?;
                    }
                }
                if let Some(body) = else_body {
                    out.push_str(&format!("{pad}}} else {{\n"));
                    for s in body {
                        self.stmt(s, level + 1, out)?;
                    }
                }
                out.push_str(&format!("{pad}}}\n"));
            }
            Stmt::For {
                var, iter, body, ..
            } => {
                out.push_str(&format!(
                    "{pad}for item in iter_value(&{})? {{\n",
                    self.expr(iter)?
                ));
                let inner = "    ".repeat(level + 1);
                out.push_str(&format!("{inner}{NAME_PREFIX}{var} = item;\n"));
                for s in body {
                    self.stmt(s, level + 1, out)?;
                }
                out.push_str(&format!("{pad}}}\n"));
            }
            Stmt::While { cond, body, span } => {
                out.push_str(&format!("{pad}loop {{\n"));
                let inner = "    ".repeat(level + 1);
                out.push_str(&format!("{inner}*cur_line = {};\n", span.line));
                out.push_str(&format!(
                    "{inner}if !({}).truthy() {{ break }}\n",
                    self.expr(cond)?
                ));
                for s in body {
                    self.stmt(s, level + 1, out)?;
                }
                out.push_str(&format!("{pad}}}\n"));
            }
            Stmt::Bork { .. } => out.push_str(&format!("{pad}break;\n")),
            Stmt::Continue { .. } => out.push_str(&format!("{pad}continue;\n")),
            Stmt::ExprStmt { expr } => {
                out.push_str(&format!("{pad}let _ = {};\n", self.expr(expr)?));
            }
            Stmt::FuncDef { span, .. } => return Err(self.unsupported(*span, Unsupported::FuncDef)),
            Stmt::Return { span, .. } => return Err(self.unsupported(*span, Unsupported::Return)),
            Stmt::Try { span, .. } => return Err(self.unsupported(*span, Unsupported::Try)),
            Stmt::ConstDecl { span, .. } => {
                return Err(self.unsupported(*span, Unsupported::ConstDecl))
            }
            Stmt::Import { span, .. } => return Err(self.unsupported(*span, Unsupported::Import)),
            Stmt::ObjDef { span, .. } => return Err(self.unsupported(*span, Unsupported::ObjDef)),
        }
        Ok(())
    }

    /// Codegen an expression to a Rust expression string. Every fallible runtime
    /// call carries a trailing `?`; we are always inside `run`, which returns
    /// `DogeResult<()>`.
    fn expr(&self, expr: &Expr) -> Result<String, Diagnostic> {
        match expr {
            Expr::Int { value, .. } => Ok(format!("Value::Int({value}i64)")),
            Expr::Float { value, .. } => Ok(format!("Value::Float({value:?}f64)")),
            Expr::Str { value, .. } => Ok(format!("Value::str(\"{}\")", escape_str(value))),
            Expr::Bool { value, .. } => Ok(format!("Value::Bool({value})")),
            Expr::None { .. } => Ok("Value::None".to_string()),
            Expr::Ident { name, .. } => Ok(format!("{NAME_PREFIX}{name}.clone()")),
            Expr::List { items, .. } => {
                let mut parts = Vec::with_capacity(items.len());
                for item in items {
                    parts.push(self.expr(item)?);
                }
                Ok(format!("Value::list(vec![{}])", parts.join(", ")))
            }
            Expr::Dict { entries, .. } => {
                let mut pairs = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    pairs.push(format!("({}, {})", self.expr(key)?, self.expr(value)?));
                }
                Ok(format!(
                    "Value::dict_from_pairs(vec![{}])?",
                    pairs.join(", ")
                ))
            }
            Expr::Binary { op, lhs, rhs, .. } => self.binary(*op, lhs, rhs),
            Expr::Unary { op, operand, .. } => {
                let inner = self.expr(operand)?;
                Ok(match op {
                    UnOp::Neg => format!("neg({inner})?"),
                    UnOp::Not => format!("not_({inner})?"),
                })
            }
            Expr::Index { obj, index, .. } => Ok(format!(
                "index_get(&{}, &{})?",
                self.expr(obj)?,
                self.expr(index)?
            )),
            Expr::Call { callee, args, span } => self.call(callee, args, *span),
            Expr::Attr { span, .. } => Err(self.unsupported(*span, Unsupported::Attr)),
        }
    }

    fn binary(&self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Result<String, Diagnostic> {
        // `and`/`or` are Rust block expressions with the right operand INSIDE the
        // guard, so it is evaluated only when the left operand doesn't decide the
        // result. Both always yield a Bool (Doge rule, DESIGN §4.2).
        if matches!(op, BinOp::And | BinOp::Or) {
            let l = self.expr(lhs)?;
            let r = self.expr(rhs)?;
            return Ok(match op {
                BinOp::And => format!(
                    "{{ let l = {l}; if !l.truthy() {{ Value::Bool(false) }} else {{ Value::Bool(({r}).truthy()) }} }}"
                ),
                BinOp::Or => format!(
                    "{{ let l = {l}; if l.truthy() {{ Value::Bool(true) }} else {{ Value::Bool(({r}).truthy()) }} }}"
                ),
                _ => unreachable!(),
            });
        }
        let l = self.expr(lhs)?;
        let r = self.expr(rhs)?;
        let func = match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
            BinOp::FloorDiv => "floordiv",
            BinOp::Rem => "rem",
            BinOp::Eq => "eq",
            BinOp::NotEq => "ne",
            BinOp::Lt => "lt",
            BinOp::LtEq => "le",
            BinOp::Gt => "gt",
            BinOp::GtEq => "ge",
            BinOp::And | BinOp::Or => unreachable!("handled above"),
        };
        Ok(format!("{func}({l}, {r})?"))
    }

    fn call(&self, callee: &Expr, args: &[Expr], span: Span) -> Result<String, Diagnostic> {
        let name = match callee {
            Expr::Ident { name, .. } if BUILTINS.contains(&name.as_str()) => name.as_str(),
            _ => return Err(self.unsupported(span, Unsupported::Call)),
        };
        self.check_arity(name, args, span)?;
        match name {
            "len" => Ok(format!("len(&{})?", self.expr(&args[0])?)),
            "str" => Ok(format!("to_str(&{})", self.expr(&args[0])?)),
            "int" => Ok(format!("to_int(&{})?", self.expr(&args[0])?)),
            "float" => Ok(format!("to_float(&{})?", self.expr(&args[0])?)),
            "range" if args.len() == 1 => Ok(format!(
                "range(&Value::Int(0i64), &{})?",
                self.expr(&args[0])?
            )),
            "range" => Ok(format!(
                "range(&{}, &{})?",
                self.expr(&args[0])?,
                self.expr(&args[1])?
            )),
            _ => unreachable!("compiler bug: arity check admitted a non-builtin"),
        }
    }

    /// Builtin arity is statically known: `len`/`str`/`int`/`float` take one
    /// argument, `range` takes one or two.
    fn check_arity(&self, name: &str, args: &[Expr], span: Span) -> Result<(), Diagnostic> {
        let (ok, expects, hint) = match name {
            "range" => (
                args.len() == 1 || args.len() == 2,
                "1 or 2 arguments",
                "range(n) or range(a, b)".to_string(),
            ),
            _ => (args.len() == 1, "1 argument", format!("{name}(thing)")),
        };
        if ok {
            return Ok(());
        }
        Err(self
            .diag(span, format!("{name} takes {expects}, got {}", args.len()))
            .with_headline(ARITY_HEADLINE)
            .with_hint(hint))
    }

    fn unsupported(&self, span: Span, feature: Unsupported) -> Diagnostic {
        let (message, milestone) = feature.detail();
        self.diag(span, message)
            .with_headline(UNSUPPORTED_HEADLINE)
            .with_hint(format!(
                "doge check already understands this script — running it lands in {milestone}"
            ))
    }

    fn diag(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        let source_line = self
            .lines
            .get((span.line as usize).saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        Diagnostic::new(&self.path, span.line, span.col, source_line, message)
    }
}

/// Every `such` name and `for` loop variable anywhere in the tree, first-seen
/// order preserved, each appearing once. These become the hoisted locals.
fn hoisted_names(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    collect_hoisted(stmts, &mut names);
    names
}

fn collect_hoisted(stmts: &[Stmt], names: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Decl { name, .. } => push_unique(names, name),
            Stmt::For { var, body, .. } => {
                push_unique(names, var);
                collect_hoisted(body, names);
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (_, body) in branches {
                    collect_hoisted(body, names);
                }
                if let Some(body) = else_body {
                    collect_hoisted(body, names);
                }
            }
            Stmt::While { body, .. } => collect_hoisted(body, names),
            _ => {}
        }
    }
}

fn push_unique(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
    }
}

/// The 1-based source line a statement points at, for `*cur_line` tracking.
fn stmt_line(stmt: &Stmt) -> u32 {
    match stmt {
        Stmt::Decl { span, .. }
        | Stmt::ConstDecl { span, .. }
        | Stmt::Import { span, .. }
        | Stmt::Assign { span, .. }
        | Stmt::Bark { span, .. }
        | Stmt::If { span, .. }
        | Stmt::For { span, .. }
        | Stmt::While { span, .. }
        | Stmt::FuncDef { span, .. }
        | Stmt::ObjDef { span, .. }
        | Stmt::Try { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Bork { span }
        | Stmt::Continue { span } => span.line,
        Stmt::ExprStmt { expr } => expr.span().line,
    }
}

/// Escape a string so it is a valid Rust string-literal body: backslash, quote,
/// newline and tab become their escape sequences. Used for both Str literals and
/// the embedded script path.
fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn gen(source: &str) -> Result<String, Diagnostic> {
        let script = parse("examples/hello.doge", source).expect("parse should succeed");
        generate("examples/hello.doge", source, &script)
    }

    #[test]
    fn golden_hello_output() {
        let out = gen("such age = 7\nbark \"age is \" + str(age)\nwow\n").unwrap();
        let expected = "\
#![allow(warnings)]
use doge_runtime::*;

fn main() -> std::process::ExitCode {
    let mut cur_line: u32 = 0;
    match run(&mut cur_line) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!(\"very error. much broken.\\n\\n  examples/hello.doge:{cur_line}\\n  {e}\");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cur_line: &mut u32) -> DogeResult<()> {
    let mut v_age: Value = Value::None;
    *cur_line = 1;
    v_age = Value::Int(7i64);
    *cur_line = 2;
    let _ = bark(&add(Value::str(\"age is \"), to_str(&v_age.clone()))?);
    Ok(())
}
";
        assert_eq!(out, expected);
    }

    #[test]
    fn decl_inside_if_is_hoisted() {
        let out = gen("such c = 1\nif c:\n    such y = 2\nbark y\nwow\n").unwrap();
        // The inner `such y` becomes a hoisted local, usable after the block.
        assert!(out.contains("    let mut v_y: Value = Value::None;\n"));
        assert!(out.contains("v_y = Value::Int(2i64);"));
        assert!(out.contains("let _ = bark(&v_y.clone());"));
    }

    #[test]
    fn for_variable_is_hoisted() {
        let out = gen("such xs = [1, 2]\nfor x in xs:\n    bark x\nwow\n").unwrap();
        assert!(out.contains("    let mut v_x: Value = Value::None;\n"));
        assert!(out.contains("for item in iter_value(&v_xs.clone())? {"));
        assert!(out.contains("v_x = item;"));
    }

    #[test]
    fn and_or_short_circuit_shape() {
        let and = gen("such a = true\nsuch b = false\nbark a and b\nwow\n").unwrap();
        assert!(and.contains(
            "{ let l = v_a.clone(); if !l.truthy() { Value::Bool(false) } else { Value::Bool((v_b.clone()).truthy()) } }"
        ));
        let or = gen("such a = true\nsuch b = false\nbark a or b\nwow\n").unwrap();
        assert!(or.contains(
            "{ let l = v_a.clone(); if l.truthy() { Value::Bool(true) } else { Value::Bool((v_b.clone()).truthy()) } }"
        ));
    }

    #[test]
    fn rust_keyword_idents_are_mangled() {
        // `match` is a Rust keyword; the `v_` prefix keeps the generated code legal.
        let out = gen("such match = 1\nbark match\nwow\n").unwrap();
        assert!(out.contains("let mut v_match: Value = Value::None;"));
        assert!(out.contains("v_match = Value::Int(1i64);"));
    }

    #[test]
    fn string_escapes_survive() {
        let out = gen("such s = \"a\\\"b\\nc\"\nwow\n").unwrap();
        // The Doge string a"b<newline>c becomes an escaped Rust string literal.
        assert!(out.contains("Value::str(\"a\\\"b\\nc\")"));
    }

    #[test]
    fn builtin_arity_error_is_precise() {
        let err = gen("bark len(1, 2, 3)\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very args. much wrong.");
        assert_eq!(err.message, "len takes 1 argument, got 3");
        assert_eq!(err.hint.as_deref(), Some("len(thing)"));

        let range_err = gen("bark range(1, 2, 3)\nwow\n").unwrap_err();
        assert_eq!(range_err.message, "range takes 1 or 2 arguments, got 3");
    }

    #[test]
    fn range_one_and_two_args() {
        let one = gen("for i in range(3):\n    bark i\nwow\n").unwrap();
        assert!(one.contains("range(&Value::Int(0i64), &Value::Int(3i64))?"));
        let two = gen("for i in range(2, 5):\n    bark i\nwow\n").unwrap();
        assert!(two.contains("range(&Value::Int(2i64), &Value::Int(5i64))?"));
    }

    #[test]
    fn unsupported_features_say_soon() {
        // (source, milestone that lands it)
        let cases = [
            ("such f:\n    return 1\nwow\nwow\n", "M4"),
            ("so PI = 3\nwow\n", "M4"),
            ("so math\nwow\n", "M4"),
            ("pls\n    bark 1\noh no err!\n    bark 2\nwow\n", "M4"),
            (
                "many Shibe:\n    such speak:\n        bark 1\n    wow\nwow\nwow\n",
                "M5",
            ),
        ];
        for (source, milestone) in cases {
            let script = parse("t.doge", source).expect("parse should succeed");
            let err = match generate("t.doge", source, &script) {
                Err(diag) => diag,
                Ok(_) => panic!("{source:?} should be unsupported"),
            };
            assert_eq!(err.headline, "very soon. much roadmap.");
            assert!(
                err.hint.as_deref().unwrap_or_default().contains(milestone),
                "expected hint to mention {milestone} for {source:?}"
            );
        }
    }

    #[test]
    fn calling_a_user_function_is_unsupported() {
        let err = gen("such greet:\n    bark 1\nwow\ngreet()\nwow\n").unwrap_err();
        // The FuncDef is reached first, so this is the funcdef message; a bare
        // call to a name is exercised where no def precedes it.
        assert_eq!(err.headline, "very soon. much roadmap.");
    }
}
