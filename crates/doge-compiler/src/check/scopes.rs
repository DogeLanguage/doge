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
/// function's body. Nested-function names are handled separately. Descent into
/// child blocks goes through the shared [`for_each_child_block`] walker, so a new
/// block-carrying statement is covered without editing this function.
pub(super) fn collect_var_bindings(body: &[Stmt], out: &mut HashSet<String>) {
    for stmt in body {
        match stmt {
            Stmt::Decl { name, .. } | Stmt::ConstDecl { name, .. } => {
                out.insert(name.clone());
            }
            Stmt::For { var, .. } => {
                out.insert(var.clone());
            }
            Stmt::Try { err_name, .. } => {
                out.insert(err_name.clone());
            }
            _ => {}
        }
        for_each_child_block(stmt, &mut |block| collect_var_bindings(block, out));
    }
}

pub(super) fn collect_nested_funcs<'a>(body: &'a [Stmt], out: &mut Vec<(&'a str, Span)>) {
    for stmt in body {
        if let Stmt::FuncDef { name, span, .. } = stmt {
            out.push((name, *span));
        }
        for_each_child_block(stmt, &mut |block| collect_nested_funcs(block, out));
    }
}
