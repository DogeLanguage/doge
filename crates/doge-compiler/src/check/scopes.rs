use super::*;

/// Every nested function defined directly in a body — crossing `if`/`for`/
/// `while`/`pls` blocks but not another function's body — with its name span.
pub(super) fn nested_funcs_with_span(body: &[Stmt]) -> Vec<(&str, Span)> {
    let mut out = Vec::new();
    collect_nested_funcs(body, &mut out);
    out
}

pub(super) fn nested_func_names(body: &[Stmt]) -> HashSet<String> {
    nested_funcs_with_span(body)
        .into_iter()
        .map(|(name, _)| name.to_string())
        .collect()
}

/// The variable-like bindings of one scope — `such`/`so` declarations, `for`
/// loop variables, and `oh no` error names — crossing blocks but not another
/// function's body. Nested-function names are handled separately.
pub(super) fn collect_var_bindings(body: &[Stmt], out: &mut HashSet<String>) {
    for stmt in body {
        match stmt {
            Stmt::Decl { name, .. } | Stmt::ConstDecl { name, .. } => {
                out.insert(name.clone());
            }
            Stmt::For { var, body, .. } => {
                out.insert(var.clone());
                collect_var_bindings(body, out);
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (_, b) in branches {
                    collect_var_bindings(b, out);
                }
                if let Some(b) = else_body {
                    collect_var_bindings(b, out);
                }
            }
            Stmt::While { body, .. } => collect_var_bindings(body, out),
            Stmt::Try {
                body,
                err_name,
                handler,
                ..
            } => {
                out.insert(err_name.clone());
                collect_var_bindings(body, out);
                collect_var_bindings(handler, out);
            }
            _ => {}
        }
    }
}

pub(super) fn collect_nested_funcs<'a>(body: &'a [Stmt], out: &mut Vec<(&'a str, Span)>) {
    for stmt in body {
        match stmt {
            Stmt::FuncDef { name, span, .. } => out.push((name, *span)),
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (_, b) in branches {
                    collect_nested_funcs(b, out);
                }
                if let Some(b) = else_body {
                    collect_nested_funcs(b, out);
                }
            }
            Stmt::For { body, .. } | Stmt::While { body, .. } => collect_nested_funcs(body, out),
            Stmt::Try { body, handler, .. } => {
                collect_nested_funcs(body, out);
                collect_nested_funcs(handler, out);
            }
            _ => {}
        }
    }
}
