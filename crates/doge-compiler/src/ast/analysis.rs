//! Pure AST analysis shared by the compiler's codegen pass and the tree-walking
//! interpreter (`doge-interp`): the free-name and nested-function queries that
//! drive closure capture. They live here, next to the AST, so neither consumer
//! depends on the other.

use std::collections::HashSet;

use super::{for_each_child_block, hoisted_names, Expr, InterpPart, Params, Span, Stmt};

/// Every identifier referenced anywhere in an expression (attribute names are
/// dynamic, not variables, so they are not collected).
pub fn expr_idents(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Ident { name, .. } => {
            out.insert(name.clone());
        }
        Expr::List { items, .. } => {
            for item in items {
                expr_idents(item, out);
            }
        }
        Expr::Dict { entries, .. } => {
            for (key, value) in entries {
                expr_idents(key, out);
                expr_idents(value, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_idents(lhs, out);
            expr_idents(rhs, out);
        }
        Expr::Unary { operand, .. } => expr_idents(operand, out),
        Expr::Call { callee, args, .. } => {
            expr_idents(callee, out);
            for arg in args {
                expr_idents(arg, out);
            }
        }
        Expr::Index { obj, index, .. } => {
            expr_idents(obj, out);
            expr_idents(index, out);
        }
        Expr::Slice {
            obj,
            start,
            end,
            step,
            ..
        } => {
            expr_idents(obj, out);
            for part in [start, end, step].into_iter().flatten() {
                expr_idents(part, out);
            }
        }
        Expr::Ternary {
            cond,
            then,
            otherwise,
            ..
        } => {
            expr_idents(cond, out);
            expr_idents(then, out);
            expr_idents(otherwise, out);
        }
        Expr::Attr { obj, .. } => expr_idents(obj, out),
        Expr::SuperCall { args, .. } => {
            for arg in args {
                expr_idents(arg, out);
            }
        }
        Expr::StrInterp { parts, .. } => {
            for part in parts {
                if let InterpPart::Expr(hole) = part {
                    expr_idents(hole, out);
                }
            }
        }
        // Literals reference no names; listed explicitly so a new expression that
        // can carry an identifier cannot be silently dropped from capture analysis.
        Expr::Int { .. }
        | Expr::Float { .. }
        | Expr::Str { .. }
        | Expr::Bool { .. }
        | Expr::None { .. } => {}
    }
}

/// Names referenced in a body, plus the free names of every nested function it
/// contains (which the enclosing scope must supply). Does not descend into a
/// nested function's own body — that is folded in through its free set.
pub fn collect_used(stmts: &[Stmt], used: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Decl { expr, .. }
            | Stmt::ConstDecl { expr, .. }
            | Stmt::Bark { expr, .. }
            | Stmt::Bonk { expr, .. }
            | Stmt::ExprStmt { expr } => expr_idents(expr, used),
            Stmt::Assign {
                targets,
                rest,
                expr,
                ..
            } => {
                for target in targets {
                    expr_idents(target, used);
                }
                if let Some(rest) = rest {
                    expr_idents(rest, used);
                }
                expr_idents(expr, used);
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (cond, body) in branches {
                    expr_idents(cond, used);
                    collect_used(body, used);
                }
                if let Some(body) = else_body {
                    collect_used(body, used);
                }
            }
            Stmt::For { iter, body, .. } => {
                expr_idents(iter, used);
                collect_used(body, used);
            }
            Stmt::While { cond, body, .. } => {
                expr_idents(cond, used);
                collect_used(body, used);
            }
            Stmt::Try { body, handler, .. } => {
                collect_used(body, used);
                collect_used(handler, used);
            }
            Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    expr_idents(expr, used);
                }
            }
            Stmt::Amaze { cond, message, .. } => {
                expr_idents(cond, used);
                if let Some(message) = message {
                    expr_idents(message, used);
                }
            }
            Stmt::FuncDef { params, body, .. } => {
                for name in free_names(&params.binding_names(), body) {
                    used.insert(name);
                }
            }
            // Reference no names from the enclosing scope; listed explicitly so a
            // new statement that can is not silently missed by capture analysis.
            Stmt::Import { .. }
            | Stmt::Bork { .. }
            | Stmt::Continue { .. }
            | Stmt::ObjDef { .. } => {}
        }
    }
}

/// The names a function body references but does not bind — the names it needs
/// from an enclosing scope (or that resolve to globals/builtins).
pub fn free_names(params: &[String], body: &[Stmt]) -> HashSet<String> {
    let mut bound: HashSet<String> = params.iter().cloned().collect();
    for name in hoisted_names(body) {
        bound.insert(name);
    }
    let mut used = HashSet::new();
    collect_used(body, &mut used);
    used.retain(|name| !bound.contains(name));
    used
}

/// The nested functions defined directly in this scope — crossing `if`/`for`/
/// `while`/`pls` blocks (names leak, Python-style) but never another function's
/// body. Returns each `(name, params, body, span)`.
pub fn child_funcdefs(stmts: &[Stmt]) -> Vec<(&str, &Params, &[Stmt], Span)> {
    let mut out = Vec::new();
    collect_child_funcdefs(stmts, &mut out);
    out
}

pub fn collect_child_funcdefs<'a>(
    stmts: &'a [Stmt],
    out: &mut Vec<(&'a str, &'a Params, &'a [Stmt], Span)>,
) {
    for stmt in stmts {
        if let Stmt::FuncDef {
            name,
            params,
            body,
            span,
        } = stmt
        {
            out.push((name, params, body, *span));
        }
        for_each_child_block(stmt, &mut |block| collect_child_funcdefs(block, out));
    }
}

/// The subset of a scope's own bound names that must be shared cells: every
/// nested-function name, plus any local or parameter a nested closure captures.
pub fn celled_locals(params: &[String], body: &[Stmt]) -> HashSet<String> {
    let mut candidates: HashSet<String> = params.iter().cloned().collect();
    for name in hoisted_names(body) {
        candidates.insert(name);
    }
    let mut child_free: HashSet<String> = HashSet::new();
    for (_, child_params, child_body, _) in child_funcdefs(body) {
        for name in free_names(&child_params.binding_names(), child_body) {
            child_free.insert(name);
        }
    }
    let funcnames: HashSet<&str> = child_funcdefs(body)
        .iter()
        .map(|(name, _, _, _)| *name)
        .collect();
    candidates
        .into_iter()
        .filter(|name| funcnames.contains(name.as_str()) || child_free.contains(name))
        .collect()
}
