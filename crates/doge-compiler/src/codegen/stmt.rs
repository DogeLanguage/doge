use super::*;

impl Codegen {
    pub(super) fn stmt(
        &self,
        stmt: &Stmt,
        level: usize,
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let pad = "    ".repeat(level);
        out.push_str(&format!("{pad}env.cur_line = {};\n", stmt.span().line));
        if self.multifile {
            out.push_str(&format!("{pad}env.cur_file = {};\n", emit.file_id));
        }
        match stmt {
            Stmt::Decl { name, expr, .. } | Stmt::ConstDecl { name, expr, .. } => {
                let value = self.expr(expr, emit)?;
                out.push_str(&format!("{pad}{}\n", self.emit_bind(emit, name, &value)));
            }
            Stmt::Assign {
                target, expr, span, ..
            } => match target {
                Expr::Ident { name, .. } => {
                    let value = self.expr(expr, emit)?;
                    self.check_writable(emit, name, *span)?;
                    out.push_str(&format!("{pad}{}\n", self.emit_bind(emit, name, &value)));
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
                Expr::Attr {
                    obj, name, span, ..
                } => {
                    if let Expr::Ident { name: base, .. } = obj.as_ref() {
                        if emit.module(base).is_some() || emit.user_module(base).is_some() {
                            return Err(self
                                .diag(*span, "cannot assign into a module")
                                .with_headline("very module. much fixed.")
                                .with_hint("a module's members are read-only"));
                        }
                    }
                    let call = format!(
                        "attr_set(&{}, \"{}\", {})",
                        self.expr(obj, emit)?,
                        escape_str(name),
                        self.expr(expr, emit)?
                    );
                    out.push_str(&format!("{pad}{};\n", self.fail(emit, call)));
                }
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
                out.push_str(&format!("{inner}{}\n", self.emit_bind(emit, var, "item")));
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
                if self.multifile {
                    out.push_str(&format!("{inner}env.cur_file = {};\n", emit.file_id));
                }
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
                out.push_str(&format!(
                    "{inner}{}\n",
                    self.emit_bind(emit, err_name, "error_value(&e)")
                ));
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
            Stmt::FuncDef { name, span, .. } => {
                let info = self.fn_info(emit, *span);
                let id = emit
                    .analysis
                    .fn_info
                    .get(&(emit.file_id, *span))
                    .and_then(|i| i.fn_id)
                    .expect("compiler bug: nested function without an id");
                emit.materialized.borrow_mut().insert(id);
                let caps: Vec<String> = info
                    .captures
                    .iter()
                    .map(|c| format!("{NAME_PREFIX}{c}.clone()"))
                    .collect();
                let value = format!(
                    "Value::function({id}u32, \"{}\", vec![{}])",
                    escape_str(name),
                    caps.join(", ")
                );
                out.push_str(&format!("{pad}{}\n", self.emit_bind(emit, name, &value)));
            }
            Stmt::Import { module, span } => {
                return Err(self
                    .diag(*span, "so imports live at the top of the script")
                    .with_headline("very nested. much import.")
                    .with_hint(format!("move so {module} to the top level")))
            }
            Stmt::ObjDef { name, span, .. } => {
                return Err(self
                    .diag(*span, "define this object at the top level")
                    .with_headline("very nested. much object.")
                    .with_hint(format!("move many {name} out to the top level")))
            }
        }
        Ok(())
    }

    /// A binding statement `name = value` (or the equivalent cell/env write). A
    /// `Cell` local is written through `cell_set`; a plain local or an `Env` field
    /// is a direct assignment.
    pub(super) fn emit_bind(&self, emit: &Emit, name: &str, value: &str) -> String {
        match emit.locals.get(name) {
            Some(Local::Cell) => format!("cell_set(&{NAME_PREFIX}{name}, {value});"),
            Some(Local::Plain) => format!("{NAME_PREFIX}{name} = {value};"),
            None => format!("env.{} = {value};", field_name(emit.file_id, name)),
        }
    }

    /// Verify an assignment target name is writable: a function, class, or module
    /// name is a fixed binding, not a variable.
    pub(super) fn check_writable(
        &self,
        emit: &Emit,
        name: &str,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if emit.local_funcs.contains_key(name) {
            return Err(self
                .diag(
                    span,
                    format!("{name} is a function — it cannot be reassigned"),
                )
                .with_headline("very function. much fixed.")
                .with_hint("pick a different variable name"));
        }
        if emit.locals.contains_key(name) {
            return Ok(());
        }
        if emit.table().funcs.contains_key(name) {
            return Err(self
                .diag(
                    span,
                    format!("{name} is a function — it cannot be reassigned"),
                )
                .with_headline("very function. much fixed.")
                .with_hint("pick a different variable name"));
        }
        if emit.class(name).is_some() {
            return Err(self
                .diag(
                    span,
                    format!("{name} is an object definition — it cannot be reassigned"),
                )
                .with_headline("very object. much fixed.")
                .with_hint("pick a different variable name"));
        }
        if emit.module(name).is_some() || emit.user_module(name).is_some() {
            return Err(self
                .diag(
                    span,
                    format!("{name} is a module — it cannot be reassigned"),
                )
                .with_headline("very module. much fixed.")
                .with_hint("pick a different variable name"));
        }
        Ok(())
    }

    /// The Rust expression that reads a name currently in scope: a plain local
    /// clones, a `Cell` local reads through `cell_get`, and anything else is an
    /// `Env` field.
    pub(super) fn resolve_read(&self, emit: &Emit, name: &str) -> String {
        match emit.locals.get(name) {
            Some(Local::Cell) => format!("cell_get(&{NAME_PREFIX}{name})"),
            Some(Local::Plain) => format!("{NAME_PREFIX}{name}.clone()"),
            None => format!("env.{}.clone()", field_name(emit.file_id, name)),
        }
    }

    /// Wrap a fallible runtime call so a failure propagates correctly: a plain
    /// `?` at the function level, or a break to the innermost `pls` label when
    /// inside a try body (so `oh no` can catch it instead of unwinding the call).
    pub(super) fn fail(&self, emit: &Emit, call: String) -> String {
        match emit.try_stack.last() {
            Some(label) => {
                format!("(match {call} {{ Ok(v) => v, Err(e) => break 'p{label} Err(e) }})")
            }
            None => format!("{call}?"),
        }
    }
}
