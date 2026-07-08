//! Doge AST → Rust source. The generated Rust is thin glue: every
//! value operation calls a function in `doge-runtime`, so behaviour lives there,
//! not in these strings.

use std::collections::{HashMap, HashSet};

use crate::ast::{BinOp, Expr, Script, Stmt, UnOp};
use crate::check::BUILTINS;
use crate::diagnostics::Diagnostic;
use crate::token::Span;

const UNSUPPORTED_HEADLINE: &str = "very soon. much roadmap.";
const ARITY_HEADLINE: &str = "very args. much wrong.";
const RUNTIME_ERROR_HEADLINE: &str = "very error. much broken.";

/// Prefix on every generated variable identifier — makes Rust-keyword
/// collisions impossible. Never appears in anything the user sees.
const NAME_PREFIX: &str = "v_";
/// Prefix on a function's outer wrapper (`f_greet`): guards recursion depth.
const FUNC_PREFIX: &str = "f_";
/// Prefix on a function's body (`b_greet`): the compiled statements. A distinct
/// prefix so a user function named `greet` and one named `b_greet` never clash.
const FUNC_BODY_PREFIX: &str = "b_";

/// Turn a checked [`Script`] into a complete Rust source file, or a diagnostic
/// pointing at the first feature M4 cannot run yet. `path`/`source` are used to
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

/// The mutable state threaded through one function's (or `run`'s) emission.
struct Emit<'a> {
    /// Every top-level function name → its parameter names (for call resolution
    /// and arity checks).
    funcs: &'a HashMap<String, Vec<String>>,
    /// Names local to the code being emitted: params plus body-hoisted names.
    /// Empty while emitting `run`, where every bound name is an `Env` field.
    locals: HashSet<String>,
    /// Monotonic per-file counter naming `'pN` try labels, `attemptN` binders,
    /// and `'lN` loop labels.
    counter: u32,
    /// Enclosing `pls` labels, innermost last: a fallible call diverts to the
    /// last one instead of `?`-ing out of the function.
    try_stack: Vec<u32>,
    /// Enclosing loop labels, innermost last: `bork`/`continue` target the last.
    loop_stack: Vec<u32>,
}

/// A language feature the parser accepts but M4 cannot run yet, with the exact
/// message and the milestone that lands it.
enum Unsupported {
    Import,
    ObjDef,
    Attr,
    NestedFuncDef,
    FuncAsValue(String),
    CallIndirect,
}

impl Unsupported {
    fn detail(&self) -> (String, &'static str) {
        match self {
            Unsupported::Import => ("so imports don't run yet — they land in M5".into(), "M5"),
            Unsupported::ObjDef => ("many objects don't run yet — they land in M5".into(), "M5"),
            Unsupported::Attr => ("object attributes land in M5".into(), "M5"),
            Unsupported::NestedFuncDef => (
                "functions inside functions land in M6 — define this function at the top level"
                    .into(),
                "M6",
            ),
            Unsupported::FuncAsValue(name) => (
                format!("{name} is a function — passing functions as values lands in M6"),
                "M6",
            ),
            Unsupported::CallIndirect => (
                "doge can only call function names and builtins for now — first-class calls land in M6"
                    .into(),
                "M6",
            ),
        }
    }
}

impl Codegen {
    fn file(&self, script: &Script) -> Result<String, Diagnostic> {
        // Pre-pass: every top-level function's signature, so a call can resolve
        // (and check its arity) before or after the definition line.
        let mut funcs: HashMap<String, Vec<String>> = HashMap::new();
        for stmt in &script.stmts {
            if let Stmt::FuncDef { name, params, .. } = stmt {
                funcs.insert(name.clone(), params.clone());
            }
        }

        // The `Env` holds the line tracker, the recursion depth, and every
        // top-level bound name — so a function can read and reassign them.
        let env_fields = hoisted_names(&script.stmts);

        let mut emit = Emit {
            funcs: &funcs,
            locals: HashSet::new(),
            counter: 0,
            try_stack: Vec::new(),
            loop_stack: Vec::new(),
        };

        // `run` holds the top-level statements; a top-level function definition
        // emits nothing here — it becomes an `f_`/`b_` pair below.
        let mut run_body = String::new();
        for stmt in &script.stmts {
            if matches!(stmt, Stmt::FuncDef { .. }) {
                continue;
            }
            self.stmt(stmt, 1, &mut emit, &mut run_body)?;
        }

        let mut funcs_src = String::new();
        for stmt in &script.stmts {
            if let Stmt::FuncDef {
                name, params, body, ..
            } = stmt
            {
                self.function(name, params, body, &mut emit, &mut funcs_src)?;
            }
        }

        let mut out = String::new();
        out.push_str("#![allow(warnings)]\n");
        out.push_str("use doge_runtime::*;\n\n");

        out.push_str("struct Env {\n");
        out.push_str("    cur_line: u32,\n");
        out.push_str("    depth: usize,\n");
        for name in &env_fields {
            out.push_str(&format!("    {NAME_PREFIX}{name}: Value,\n"));
        }
        out.push_str("}\n\n");

        out.push_str("fn main() -> std::process::ExitCode {\n");
        out.push_str("    let mut env = Env {\n");
        out.push_str("        cur_line: 0,\n");
        out.push_str("        depth: 0,\n");
        for name in &env_fields {
            out.push_str(&format!("        {NAME_PREFIX}{name}: Value::None,\n"));
        }
        out.push_str("    };\n");
        out.push_str("    match run(&mut env) {\n");
        out.push_str("        Ok(()) => std::process::ExitCode::SUCCESS,\n");
        out.push_str("        Err(e) => {\n");
        out.push_str(&format!(
            "            eprintln!(\"{}\\n\\n  {}:{{}}\\n  {{e}}\", env.cur_line);\n",
            RUNTIME_ERROR_HEADLINE,
            escape_str(&self.path)
        ));
        out.push_str("            std::process::ExitCode::FAILURE\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");

        out.push_str("fn run(env: &mut Env) -> DogeResult<()> {\n");
        out.push_str(&run_body);
        out.push_str("    Ok(())\n");
        out.push_str("}\n");

        out.push_str(&funcs_src);
        Ok(out)
    }

    /// Emit a top-level function as a wrapper + body pair. The wrapper counts the
    /// call against the recursion limit and undoes it on every exit path — even a
    /// `?` inside the body — because `exit_call` runs after the body returns.
    fn function(
        &self,
        name: &str,
        params: &[String],
        body: &[Stmt],
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let wrapper_params = signature(params, false);
        out.push_str(&format!(
            "\nfn {FUNC_PREFIX}{name}({wrapper_params}) -> DogeResult<Value> {{\n"
        ));
        out.push_str("    enter_call(&mut env.depth)?;\n");
        let call_args = {
            let mut v: Vec<String> = params.iter().map(|p| format!("{NAME_PREFIX}{p}")).collect();
            v.push("env".to_string());
            v.join(", ")
        };
        out.push_str(&format!(
            "    let result = {FUNC_BODY_PREFIX}{name}({call_args});\n"
        ));
        out.push_str("    exit_call(&mut env.depth);\n");
        out.push_str("    result\n");
        out.push_str("}\n");

        let body_params = signature(params, true);
        out.push_str(&format!(
            "\nfn {FUNC_BODY_PREFIX}{name}({body_params}) -> DogeResult<Value> {{\n"
        ));

        // A function's locals are its params plus every name it hoists. Params
        // are already bound by the signature, so only the rest get a `let`.
        let hoisted = hoisted_names(body);
        let mut locals: HashSet<String> = params.iter().cloned().collect();
        for local in &hoisted {
            if !params.iter().any(|p| p == local) {
                out.push_str(&format!(
                    "    let mut {NAME_PREFIX}{local}: Value = Value::None;\n"
                ));
            }
            locals.insert(local.clone());
        }

        emit.locals = locals;
        emit.try_stack.clear();
        emit.loop_stack.clear();
        for stmt in body {
            self.stmt(stmt, 1, emit, out)?;
        }
        // Falling off the end returns none.
        out.push_str("    Ok(Value::None)\n");
        out.push_str("}\n");
        Ok(())
    }

    fn stmt(
        &self,
        stmt: &Stmt,
        level: usize,
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let pad = "    ".repeat(level);
        out.push_str(&format!("{pad}env.cur_line = {};\n", stmt_line(stmt)));
        match stmt {
            Stmt::Decl { name, expr, .. } | Stmt::ConstDecl { name, expr, .. } => {
                let value = self.expr(expr, emit)?;
                let dest = self.resolve_binding(emit, name);
                out.push_str(&format!("{pad}{dest} = {value};\n"));
            }
            Stmt::Assign {
                target, expr, span, ..
            } => match target {
                Expr::Ident { name, .. } => {
                    let value = self.expr(expr, emit)?;
                    let dest = self.resolve_write(emit, name, *span)?;
                    out.push_str(&format!("{pad}{dest} = {value};\n"));
                }
                Expr::Index { obj, index, .. } => {
                    let call = format!(
                        "index_set(&{}, &{}, {})",
                        self.expr(obj, emit)?,
                        self.expr(index, emit)?,
                        self.expr(expr, emit)?
                    );
                    out.push_str(&format!("{pad}{};\n", self.fail(emit, call)));
                }
                Expr::Attr { span, .. } => return Err(self.unsupported(*span, Unsupported::Attr)),
                _ => unreachable!("compiler bug: parser guarantees a valid assign target"),
            },
            Stmt::Bark { expr, .. } => {
                out.push_str(&format!(
                    "{pad}let _ = bark(&{});\n",
                    self.expr(expr, emit)?
                ));
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (i, (cond, body)) in branches.iter().enumerate() {
                    let head = if i == 0 { "if" } else { "} else if" };
                    out.push_str(&format!(
                        "{pad}{head} ({}).truthy() {{\n",
                        self.expr(cond, emit)?
                    ));
                    for s in body {
                        self.stmt(s, level + 1, emit, out)?;
                    }
                }
                if let Some(body) = else_body {
                    out.push_str(&format!("{pad}}} else {{\n"));
                    for s in body {
                        self.stmt(s, level + 1, emit, out)?;
                    }
                }
                out.push_str(&format!("{pad}}}\n"));
            }
            Stmt::For {
                var, iter, body, ..
            } => {
                let iter_expr = self.expr(iter, emit)?;
                let iter_call = self.fail(emit, format!("iter_value(&{iter_expr})"));
                let label = emit.counter;
                emit.counter += 1;
                out.push_str(&format!("{pad}'l{label}: for item in {iter_call} {{\n"));
                emit.loop_stack.push(label);
                let inner = "    ".repeat(level + 1);
                let dest = self.resolve_binding(emit, var);
                out.push_str(&format!("{inner}{dest} = item;\n"));
                for s in body {
                    self.stmt(s, level + 1, emit, out)?;
                }
                emit.loop_stack.pop();
                out.push_str(&format!("{pad}}}\n"));
            }
            Stmt::While { cond, body, span } => {
                let label = emit.counter;
                emit.counter += 1;
                out.push_str(&format!("{pad}'l{label}: loop {{\n"));
                emit.loop_stack.push(label);
                let inner = "    ".repeat(level + 1);
                out.push_str(&format!("{inner}env.cur_line = {};\n", span.line));
                out.push_str(&format!(
                    "{inner}if !({}).truthy() {{ break 'l{label} }}\n",
                    self.expr(cond, emit)?
                ));
                for s in body {
                    self.stmt(s, level + 1, emit, out)?;
                }
                emit.loop_stack.pop();
                out.push_str(&format!("{pad}}}\n"));
            }
            Stmt::Try {
                body,
                err_name,
                handler,
                ..
            } => {
                let label = emit.counter;
                emit.counter += 1;
                let inner = "    ".repeat(level + 1);
                out.push_str(&format!(
                    "{pad}let attempt{label}: DogeResult<()> = 'p{label}: {{\n"
                ));
                emit.try_stack.push(label);
                for s in body {
                    self.stmt(s, level + 1, emit, out)?;
                }
                emit.try_stack.pop();
                out.push_str(&format!("{inner}Ok(())\n"));
                out.push_str(&format!("{pad}}};\n"));
                out.push_str(&format!("{pad}if let Err(e) = attempt{label} {{\n"));
                let dest = self.resolve_binding(emit, err_name);
                out.push_str(&format!("{inner}{dest} = error_value(&e);\n"));
                for s in handler {
                    self.stmt(s, level + 1, emit, out)?;
                }
                out.push_str(&format!("{pad}}}\n"));
            }
            Stmt::Return { expr, .. } => {
                let value = match expr {
                    Some(expr) => self.expr(expr, emit)?,
                    None => "Value::None".to_string(),
                };
                out.push_str(&format!("{pad}return Ok({value});\n"));
            }
            Stmt::Bonk { expr, .. } => {
                let value = self.expr(expr, emit)?;
                match emit.try_stack.last() {
                    Some(label) => out.push_str(&format!(
                        "{pad}break 'p{label} Err(bonk_error(&{value}));\n"
                    )),
                    None => out.push_str(&format!("{pad}return Err(bonk_error(&{value}));\n")),
                }
            }
            Stmt::Bork { .. } => {
                let label = emit
                    .loop_stack
                    .last()
                    .expect("compiler bug: bork outside a loop reached codegen");
                out.push_str(&format!("{pad}break 'l{label};\n"));
            }
            Stmt::Continue { .. } => {
                let label = emit
                    .loop_stack
                    .last()
                    .expect("compiler bug: continue outside a loop reached codegen");
                out.push_str(&format!("{pad}continue 'l{label};\n"));
            }
            Stmt::ExprStmt { expr } => {
                out.push_str(&format!("{pad}let _ = {};\n", self.expr(expr, emit)?));
            }
            Stmt::FuncDef { span, .. } => {
                return Err(self.unsupported(*span, Unsupported::NestedFuncDef))
            }
            Stmt::Import { span, .. } => return Err(self.unsupported(*span, Unsupported::Import)),
            Stmt::ObjDef { span, .. } => return Err(self.unsupported(*span, Unsupported::ObjDef)),
        }
        Ok(())
    }

    /// The Rust place a name is *bound* to: a local, or an `Env` field. Binding
    /// a fresh name (a `such`, a loop variable, a caught error) never clashes
    /// with a function — the checker guarantees top-level names are distinct.
    fn resolve_binding(&self, emit: &Emit, name: &str) -> String {
        if emit.locals.contains(name) {
            format!("{NAME_PREFIX}{name}")
        } else {
            format!("env.{NAME_PREFIX}{name}")
        }
    }

    /// The Rust place an *assignment* writes to. Reassigning a function name is a
    /// real error: a function is a fixed binding, not a variable.
    fn resolve_write(&self, emit: &Emit, name: &str, span: Span) -> Result<String, Diagnostic> {
        if emit.locals.contains(name) {
            Ok(format!("{NAME_PREFIX}{name}"))
        } else if emit.funcs.contains_key(name) {
            Err(self
                .diag(
                    span,
                    format!("{name} is a function — it cannot be reassigned"),
                )
                .with_headline("very function. much fixed.")
                .with_hint("pick a different variable name"))
        } else {
            Ok(format!("env.{NAME_PREFIX}{name}"))
        }
    }

    /// Wrap a fallible runtime call so a failure propagates correctly: a plain
    /// `?` at the function level, or a break to the innermost `pls` label when
    /// inside a try body (so `oh no` can catch it instead of unwinding the call).
    fn fail(&self, emit: &Emit, call: String) -> String {
        match emit.try_stack.last() {
            Some(label) => {
                format!("(match {call} {{ Ok(v) => v, Err(e) => break 'p{label} Err(e) }})")
            }
            None => format!("{call}?"),
        }
    }

    /// Codegen an expression to a Rust expression string. Every fallible runtime
    /// call is routed through [`Codegen::fail`].
    fn expr(&self, expr: &Expr, emit: &Emit) -> Result<String, Diagnostic> {
        match expr {
            Expr::Int { value, .. } => Ok(format!("Value::Int({value}i64)")),
            Expr::Float { value, .. } => Ok(format!("Value::Float({value:?}f64)")),
            Expr::Str { value, .. } => Ok(format!("Value::str(\"{}\")", escape_str(value))),
            Expr::Bool { value, .. } => Ok(format!("Value::Bool({value})")),
            Expr::None { .. } => Ok("Value::None".to_string()),
            Expr::Ident { name, span } => {
                if emit.locals.contains(name) {
                    Ok(format!("{NAME_PREFIX}{name}.clone()"))
                } else if emit.funcs.contains_key(name) || BUILTINS.contains(&name.as_str()) {
                    Err(self.unsupported(*span, Unsupported::FuncAsValue(name.clone())))
                } else {
                    Ok(format!("env.{NAME_PREFIX}{name}.clone()"))
                }
            }
            Expr::List { items, .. } => {
                let mut parts = Vec::with_capacity(items.len());
                for item in items {
                    parts.push(self.expr(item, emit)?);
                }
                Ok(format!("Value::list(vec![{}])", parts.join(", ")))
            }
            Expr::Dict { entries, .. } => {
                let mut pairs = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    pairs.push(format!(
                        "({}, {})",
                        self.expr(key, emit)?,
                        self.expr(value, emit)?
                    ));
                }
                Ok(self.fail(
                    emit,
                    format!("Value::dict_from_pairs(vec![{}])", pairs.join(", ")),
                ))
            }
            Expr::Binary { op, lhs, rhs, .. } => self.binary(*op, lhs, rhs, emit),
            Expr::Unary { op, operand, .. } => {
                let inner = self.expr(operand, emit)?;
                Ok(match op {
                    UnOp::Neg => self.fail(emit, format!("neg({inner})")),
                    UnOp::Not => self.fail(emit, format!("not_({inner})")),
                })
            }
            Expr::Index { obj, index, .. } => {
                let call = format!(
                    "index_get(&{}, &{})",
                    self.expr(obj, emit)?,
                    self.expr(index, emit)?
                );
                Ok(self.fail(emit, call))
            }
            Expr::Call { callee, args, span } => self.call(callee, args, *span, emit),
            Expr::Attr { span, .. } => Err(self.unsupported(*span, Unsupported::Attr)),
        }
    }

    fn binary(&self, op: BinOp, lhs: &Expr, rhs: &Expr, emit: &Emit) -> Result<String, Diagnostic> {
        // `and`/`or` are Rust block expressions with the right operand INSIDE the
        // guard, so it is evaluated only when the left operand doesn't decide the
        // result. Both always yield a Bool (Doge rule, DESIGN §4.2).
        if matches!(op, BinOp::And | BinOp::Or) {
            let l = self.expr(lhs, emit)?;
            let r = self.expr(rhs, emit)?;
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
        let l = self.expr(lhs, emit)?;
        let r = self.expr(rhs, emit)?;
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
        Ok(self.fail(emit, format!("{func}({l}, {r})")))
    }

    fn call(
        &self,
        callee: &Expr,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        match callee {
            Expr::Ident { name, .. } if BUILTINS.contains(&name.as_str()) => {
                self.builtin_call(name, args, span, emit)
            }
            Expr::Ident { name, .. } if emit.funcs.contains_key(name) => {
                let params = &emit.funcs[name];
                self.check_user_arity(name, params, args, span)?;
                let mut parts = Vec::with_capacity(args.len() + 1);
                for arg in args {
                    parts.push(self.expr(arg, emit)?);
                }
                parts.push("&mut *env".to_string());
                let call = format!("{FUNC_PREFIX}{name}({})", parts.join(", "));
                Ok(self.fail(emit, call))
            }
            Expr::Attr { span, .. } => Err(self.unsupported(*span, Unsupported::Attr)),
            _ => Err(self.unsupported(span, Unsupported::CallIndirect)),
        }
    }

    fn builtin_call(
        &self,
        name: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        self.check_builtin_arity(name, args, span)?;
        match name {
            "len" => Ok(self.fail(emit, format!("len(&{})", self.expr(&args[0], emit)?))),
            "str" => Ok(format!("to_str(&{})", self.expr(&args[0], emit)?)),
            "int" => Ok(self.fail(emit, format!("to_int(&{})", self.expr(&args[0], emit)?))),
            "float" => Ok(self.fail(emit, format!("to_float(&{})", self.expr(&args[0], emit)?))),
            "range" if args.len() == 1 => Ok(self.fail(
                emit,
                format!("range(&Value::Int(0i64), &{})", self.expr(&args[0], emit)?),
            )),
            "range" => Ok(self.fail(
                emit,
                format!(
                    "range(&{}, &{})",
                    self.expr(&args[0], emit)?,
                    self.expr(&args[1], emit)?
                ),
            )),
            _ => unreachable!("compiler bug: arity check admitted a non-builtin"),
        }
    }

    /// Builtin arity is statically known: `len`/`str`/`int`/`float` take one
    /// argument, `range` takes one or two.
    fn check_builtin_arity(&self, name: &str, args: &[Expr], span: Span) -> Result<(), Diagnostic> {
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

    /// A user function takes exactly its declared parameters; the hint echoes the
    /// call shape doge expected.
    fn check_user_arity(
        &self,
        name: &str,
        params: &[String],
        args: &[Expr],
        span: Span,
    ) -> Result<(), Diagnostic> {
        if args.len() == params.len() {
            return Ok(());
        }
        let count = params.len();
        let noun = if count == 1 { "argument" } else { "arguments" };
        let hint = if params.is_empty() {
            format!("{name}()")
        } else {
            format!("{name}({})", params.join(", "))
        };
        Err(self
            .diag(
                span,
                format!("{name} takes {count} {noun}, got {}", args.len()),
            )
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

/// Build a comma-joined parameter list ending in the shared `env`. `owned` adds
/// `mut` so a body can reassign its parameters; the wrapper takes them plain.
fn signature(params: &[String], owned: bool) -> String {
    let mut parts: Vec<String> = params
        .iter()
        .map(|p| {
            if owned {
                format!("mut {NAME_PREFIX}{p}: Value")
            } else {
                format!("{NAME_PREFIX}{p}: Value")
            }
        })
        .collect();
    parts.push("env: &mut Env".to_string());
    parts.join(", ")
}

/// Every top-level bound name — `such`/`so` declarations, `for` loop variables,
/// and `oh no` error names — in first-seen order, each once. These become the
/// `Env` fields (or, per function, that function's hoisted locals). Function
/// definitions are not descended into: their names belong to their own scope.
pub(crate) fn hoisted_names(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    collect_hoisted(stmts, &mut names);
    names
}

fn collect_hoisted(stmts: &[Stmt], names: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Decl { name, .. } | Stmt::ConstDecl { name, .. } => push_unique(names, name),
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
            Stmt::Try {
                body,
                err_name,
                handler,
                ..
            } => {
                push_unique(names, err_name);
                collect_hoisted(body, names);
                collect_hoisted(handler, names);
            }
            _ => {}
        }
    }
}

fn push_unique(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
    }
}

/// The 1-based source line a statement points at, for `env.cur_line` tracking.
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
        | Stmt::Bonk { span, .. }
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

struct Env {
    cur_line: u32,
    depth: usize,
    v_age: Value,
}

fn main() -> std::process::ExitCode {
    let mut env = Env {
        cur_line: 0,
        depth: 0,
        v_age: Value::None,
    };
    match run(&mut env) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!(\"very error. much broken.\\n\\n  examples/hello.doge:{}\\n  {e}\", env.cur_line);
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(env: &mut Env) -> DogeResult<()> {
    env.cur_line = 1;
    env.v_age = Value::Int(7i64);
    env.cur_line = 2;
    let _ = bark(&add(Value::str(\"age is \"), to_str(&env.v_age.clone()))?);
    Ok(())
}
";
        assert_eq!(out, expected);
    }

    #[test]
    fn golden_function_shape() {
        let out = gen("such greet much name:\n    return name\nwow\nbark greet(\"kabosu\")\nwow\n")
            .unwrap();
        let expected = "\
#![allow(warnings)]
use doge_runtime::*;

struct Env {
    cur_line: u32,
    depth: usize,
}

fn main() -> std::process::ExitCode {
    let mut env = Env {
        cur_line: 0,
        depth: 0,
    };
    match run(&mut env) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!(\"very error. much broken.\\n\\n  examples/hello.doge:{}\\n  {e}\", env.cur_line);
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(env: &mut Env) -> DogeResult<()> {
    env.cur_line = 4;
    let _ = bark(&f_greet(Value::str(\"kabosu\"), &mut *env)?);
    Ok(())
}

fn f_greet(v_name: Value, env: &mut Env) -> DogeResult<Value> {
    enter_call(&mut env.depth)?;
    let result = b_greet(v_name, env);
    exit_call(&mut env.depth);
    result
}

fn b_greet(mut v_name: Value, env: &mut Env) -> DogeResult<Value> {
    env.cur_line = 2;
    return Ok(v_name.clone());
    Ok(Value::None)
}
";
        assert_eq!(out, expected);
    }

    #[test]
    fn decl_inside_if_is_hoisted() {
        let out = gen("such c = 1\nif c:\n    such y = 2\nbark y\nwow\n").unwrap();
        assert!(out.contains("    v_y: Value,\n"));
        assert!(out.contains("env.v_y = Value::Int(2i64);"));
        assert!(out.contains("let _ = bark(&env.v_y.clone());"));
    }

    #[test]
    fn for_variable_is_hoisted() {
        let out = gen("such xs = [1, 2]\nfor x in xs:\n    bark x\nwow\n").unwrap();
        assert!(out.contains("    v_x: Value,\n"));
        assert!(out.contains("'l0: for item in iter_value(&env.v_xs.clone())? {"));
        assert!(out.contains("env.v_x = item;"));
    }

    #[test]
    fn and_or_short_circuit_shape() {
        let and = gen("such a = true\nsuch b = false\nbark a and b\nwow\n").unwrap();
        assert!(and.contains(
            "{ let l = env.v_a.clone(); if !l.truthy() { Value::Bool(false) } else { Value::Bool((env.v_b.clone()).truthy()) } }"
        ));
        let or = gen("such a = true\nsuch b = false\nbark a or b\nwow\n").unwrap();
        assert!(or.contains(
            "{ let l = env.v_a.clone(); if l.truthy() { Value::Bool(true) } else { Value::Bool((env.v_b.clone()).truthy()) } }"
        ));
    }

    #[test]
    fn rust_keyword_idents_are_mangled() {
        // `match` is a Rust keyword; the `v_` prefix keeps the generated code legal.
        let out = gen("such match = 1\nbark match\nwow\n").unwrap();
        assert!(out.contains("    v_match: Value,\n"));
        assert!(out.contains("env.v_match = Value::Int(1i64);"));
    }

    #[test]
    fn string_escapes_survive() {
        let out = gen("such s = \"a\\\"b\\nc\"\nwow\n").unwrap();
        // The Doge string a"b<newline>c becomes an escaped Rust string literal.
        assert!(out.contains("Value::str(\"a\\\"b\\nc\")"));
    }

    #[test]
    fn const_compiles_like_decl() {
        let out = gen("so PI = 3\nbark PI\nwow\n").unwrap();
        assert!(out.contains("    v_PI: Value,\n"));
        assert!(out.contains("env.v_PI = Value::Int(3i64);"));
        assert!(out.contains("let _ = bark(&env.v_PI.clone());"));
    }

    #[test]
    fn try_block_shape() {
        let out =
            gen("such x = 0\npls\n    very x = 1 // 0\noh no err!\n    bark err\nwow\n").unwrap();
        assert!(out.contains("let attempt0: DogeResult<()> = 'p0: {"));
        assert!(out.contains("Err(e) => break 'p0 Err(e)"));
        assert!(out.contains("if let Err(e) = attempt0 {"));
        assert!(out.contains("env.v_err = error_value(&e);"));
    }

    #[test]
    fn bonk_returns_err() {
        let out = gen("bonk \"nope\"\nwow\n").unwrap();
        assert!(out.contains("return Err(bonk_error(&Value::str(\"nope\")));"));
    }

    #[test]
    fn bonk_in_try_breaks_to_label() {
        let out = gen("pls\n    bonk \"nope\"\noh no err!\n    bark err\nwow\n").unwrap();
        assert!(out.contains("break 'p0 Err(bonk_error(&Value::str(\"nope\")));"));
    }

    #[test]
    fn loops_are_labeled_and_bork_uses_labels() {
        // A bork inside a pls inside a for must break the labeled loop, crossing
        // the labeled try block.
        let out =
            gen("such xs = [1]\nfor x in xs:\n    pls\n        bork\n    oh no err!\n        bark err\nwow\n")
                .unwrap();
        assert!(out.contains("'l0: for item in"));
        assert!(out.contains("'p1: {"));
        assert!(out.contains("break 'l0;"));
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
    fn function_arity_error_is_precise() {
        let err =
            gen("such add2 much a, b:\n    return a + b\nwow\nbark add2(1)\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very args. much wrong.");
        assert_eq!(err.message, "add2 takes 2 arguments, got 1");
        assert_eq!(err.hint.as_deref(), Some("add2(a, b)"));
    }

    #[test]
    fn range_one_and_two_args() {
        let one = gen("for i in range(3):\n    bark i\nwow\n").unwrap();
        assert!(one.contains("range(&Value::Int(0i64), &Value::Int(3i64))?"));
        let two = gen("for i in range(2, 5):\n    bark i\nwow\n").unwrap();
        assert!(two.contains("range(&Value::Int(2i64), &Value::Int(5i64))?"));
    }

    #[test]
    fn function_as_value_lands_m6() {
        let err = gen("such greet:\n    bark 1\nwow\nsuch g = greet\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very soon. much roadmap.");
        assert!(err.hint.as_deref().unwrap_or_default().contains("M6"));
    }

    #[test]
    fn builtin_as_value_lands_m6() {
        // `bark len` — a bare builtin name used as a value.
        let err = gen("bark len\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very soon. much roadmap.");
        assert!(err.hint.as_deref().unwrap_or_default().contains("M6"));
    }

    #[test]
    fn indirect_call_lands_m6() {
        let err = gen("such x = 1\nx()\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very soon. much roadmap.");
        assert!(err.hint.as_deref().unwrap_or_default().contains("M6"));
    }

    #[test]
    fn nested_funcdef_lands_m6() {
        let err =
            gen("such outer:\n    such inner:\n        bark 1\n    wow\nwow\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very soon. much roadmap.");
        assert!(err.hint.as_deref().unwrap_or_default().contains("M6"));
    }

    #[test]
    fn import_lands_m5() {
        let err = gen("so math\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very soon. much roadmap.");
        assert!(err.hint.as_deref().unwrap_or_default().contains("M5"));
    }

    #[test]
    fn assign_to_function_name_is_an_error() {
        let err = gen("such greet:\n    bark 1\nwow\nvery greet = 5\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very function. much fixed.");
    }

    #[test]
    fn fn_local_vs_global_resolution() {
        // The function reassigns a top-level name (env field) and declares its
        // own local (a plain `v_`).
        let out = gen(
            "such total = 0\nsuch tally much n:\n    such step = n\n    very total = total + step\n    return total\nwow\nbark tally(2)\nwow\n",
        )
        .unwrap();
        assert!(out.contains("let mut v_step: Value = Value::None;"));
        assert!(out.contains("env.v_total = add(env.v_total.clone(), v_step.clone())?;"));
    }

    #[test]
    fn bare_return_and_missing_return_yield_none() {
        let out = gen("such f:\n    return\nwow\nf()\nwow\n").unwrap();
        assert!(out.contains("return Ok(Value::None);"));
        // The body still ends with the fall-off-end none.
        assert!(out.contains("    Ok(Value::None)\n}\n"));
    }
}
