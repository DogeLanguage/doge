use super::{Expr, InterpPart, Params, Script, Stmt};

/// Render a script as an indented tree (two spaces per level). Stable and
/// language-agnostic — this is what `doge check` prints on success.
pub fn dump(script: &Script) -> String {
    let mut out = String::new();
    out.push_str("Script\n");
    for stmt in &script.stmts {
        dump_stmt(stmt, 1, &mut out);
    }
    out
}

fn indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn line(level: usize, text: &str, out: &mut String) {
    indent(level, out);
    out.push_str(text);
    out.push('\n');
}

/// Render a binding-target name list for a destructuring `Decl`/`For` header:
/// the leading names comma-joined, then a trailing `many rest` collector when one
/// is present, e.g. `a, b` or `a, many rest`.
fn name_list(names: &[String], rest: Option<&str>) -> String {
    let mut parts: Vec<String> = names.to_vec();
    if let Some(rest) = rest {
        parts.push(format!("many {rest}"));
    }
    parts.join(", ")
}

fn dump_block(label: &str, body: &[Stmt], level: usize, out: &mut String) {
    line(level, label, out);
    for stmt in body {
        dump_stmt(stmt, level + 1, out);
    }
}

fn dump_stmt(stmt: &Stmt, level: usize, out: &mut String) {
    match stmt {
        Stmt::Decl {
            names, rest, expr, ..
        } => {
            line(
                level,
                &format!("Decl {}", name_list(names, rest.as_deref())),
                out,
            );
            dump_expr(expr, level + 1, out);
        }
        Stmt::ConstDecl { name, expr, .. } => {
            line(level, &format!("ConstDecl {name}"), out);
            dump_expr(expr, level + 1, out);
        }
        Stmt::Import { module, .. } => line(level, &format!("Import {module}"), out),
        Stmt::Assign {
            targets,
            rest,
            expr,
            flavored,
            ..
        } => {
            let label = if *flavored { "Assign very" } else { "Assign" };
            line(level, label, out);
            for target in targets {
                dump_block_expr("target", target, level + 1, out);
            }
            if let Some(rest) = rest {
                dump_block_expr("many target", rest, level + 1, out);
            }
            dump_block_expr("value", expr, level + 1, out);
        }
        Stmt::Bark { expr, .. } => {
            line(level, "Bark", out);
            dump_expr(expr, level + 1, out);
        }
        Stmt::If {
            branches,
            else_body,
            ..
        } => {
            line(level, "If", out);
            for (cond, body) in branches {
                line(level + 1, "branch", out);
                dump_block_expr("cond", cond, level + 2, out);
                dump_block("body", body, level + 2, out);
            }
            if let Some(body) = else_body {
                dump_block("else", body, level + 1, out);
            }
        }
        Stmt::For {
            vars,
            rest,
            iter,
            body,
            ..
        } => {
            line(
                level,
                &format!("For {}", name_list(vars, rest.as_deref())),
                out,
            );
            dump_block_expr("in", iter, level + 1, out);
            dump_block("body", body, level + 1, out);
        }
        Stmt::While { cond, body, .. } => {
            line(level, "While", out);
            dump_block_expr("cond", cond, level + 1, out);
            dump_block("body", body, level + 1, out);
        }
        Stmt::FuncDef {
            name, params, body, ..
        } => {
            let params = if params.is_empty() {
                String::new()
            } else {
                format!(" much {}", dump_params(params))
            };
            dump_block(&format!("FuncDef {name}{params}"), body, level, out);
        }
        Stmt::ObjDef {
            name,
            parent,
            methods,
            ..
        } => {
            let heading = match parent {
                Some(parent) => format!("ObjDef {name} much {parent}"),
                None => format!("ObjDef {name}"),
            };
            dump_block(&heading, methods, level, out);
        }
        Stmt::Try {
            body,
            err_name,
            handler,
            ..
        } => {
            line(level, "Try", out);
            dump_block("body", body, level + 1, out);
            dump_block(&format!("catch {err_name}"), handler, level + 1, out);
        }
        Stmt::Return { expr, .. } => match expr {
            Some(expr) => {
                line(level, "Return", out);
                dump_expr(expr, level + 1, out);
            }
            None => line(level, "Return", out),
        },
        Stmt::Bonk { expr, .. } => {
            line(level, "Bonk", out);
            dump_expr(expr, level + 1, out);
        }
        Stmt::Amaze { cond, message, .. } => {
            line(level, "Amaze", out);
            dump_expr(cond, level + 1, out);
            if let Some(message) = message {
                dump_expr(message, level + 1, out);
            }
        }
        Stmt::Bork { .. } => line(level, "Bork", out),
        Stmt::Continue { .. } => line(level, "Continue", out),
        Stmt::ExprStmt { expr } => {
            line(level, "ExprStmt", out);
            dump_expr(expr, level + 1, out);
        }
    }
}

/// Render a parameter list for the `much …` header line: `name`, `mood = …` for
/// a defaulted parameter, `many rest` for the variadic.
fn dump_params(params: &Params) -> String {
    let mut parts: Vec<String> = params
        .params
        .iter()
        .map(|p| {
            if p.default.is_some() {
                format!("{} = …", p.name)
            } else {
                p.name.clone()
            }
        })
        .collect();
    if let Some(rest) = &params.vararg {
        parts.push(format!("many {rest}"));
    }
    parts.join(", ")
}

/// Dump an expression under a named sub-heading, e.g. `cond` / `target`.
fn dump_block_expr(label: &str, expr: &Expr, level: usize, out: &mut String) {
    line(level, label, out);
    dump_expr(expr, level + 1, out);
}

fn dump_expr(expr: &Expr, level: usize, out: &mut String) {
    match expr {
        Expr::Int { value, .. } => line(level, &format!("Int {value}"), out),
        Expr::Float { value, .. } => line(level, &format!("Float {value}"), out),
        Expr::Str { value, .. } => line(level, &format!("Str {value:?}"), out),
        Expr::Bool { value, .. } => line(level, &format!("Bool {value}"), out),
        Expr::None { .. } => line(level, "None", out),
        Expr::Ident { name, .. } => line(level, &format!("Ident {name}"), out),
        Expr::List { items, .. } => {
            line(level, "List", out);
            for item in items {
                dump_expr(item, level + 1, out);
            }
        }
        Expr::Dict { entries, .. } => {
            line(level, "Dict", out);
            for (key, value) in entries {
                line(level + 1, "entry", out);
                dump_block_expr("key", key, level + 2, out);
                dump_block_expr("value", value, level + 2, out);
            }
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            line(level, &format!("Binary {}", op.symbol()), out);
            dump_expr(lhs, level + 1, out);
            dump_expr(rhs, level + 1, out);
        }
        Expr::Unary { op, operand, .. } => {
            line(level, &format!("Unary {}", op.symbol()), out);
            dump_expr(operand, level + 1, out);
        }
        Expr::Call {
            callee,
            args,
            kwargs,
            ..
        } => {
            line(level, "Call", out);
            dump_block_expr("callee", callee, level + 1, out);
            for arg in args {
                dump_block_expr("arg", arg, level + 1, out);
            }
            for (name, value) in kwargs {
                dump_block_expr(&format!("kwarg {name}"), value, level + 1, out);
            }
        }
        Expr::Index { obj, index, .. } => {
            line(level, "Index", out);
            dump_block_expr("obj", obj, level + 1, out);
            dump_block_expr("index", index, level + 1, out);
        }
        Expr::Slice {
            obj,
            start,
            end,
            step,
            ..
        } => {
            line(level, "Slice", out);
            dump_block_expr("obj", obj, level + 1, out);
            for (label, part) in [("start", start), ("end", end), ("step", step)] {
                match part {
                    Some(expr) => dump_block_expr(label, expr, level + 1, out),
                    None => line(level + 1, &format!("{label} none"), out),
                }
            }
        }
        Expr::Ternary {
            cond,
            then,
            otherwise,
            ..
        } => {
            line(level, "Ternary", out);
            dump_block_expr("cond", cond, level + 1, out);
            dump_block_expr("then", then, level + 1, out);
            dump_block_expr("else", otherwise, level + 1, out);
        }
        Expr::Attr { obj, name, .. } => {
            line(level, &format!("Attr {name}"), out);
            dump_expr(obj, level + 1, out);
        }
        Expr::SuperCall { method, args, .. } => {
            line(level, &format!("SuperCall {method}"), out);
            for arg in args {
                dump_block_expr("arg", arg, level + 1, out);
            }
        }
        Expr::StrInterp { parts, .. } => {
            line(level, "StrInterp", out);
            for part in parts {
                match part {
                    InterpPart::Lit(text) => line(level + 1, &format!("Lit {text:?}"), out),
                    InterpPart::Expr(expr) => dump_block_expr("hole", expr, level + 1, out),
                }
            }
        }
    }
}
