use super::*;

/// The item or field an augmented assignment writes back to, carrying the
/// already-emitted receiver (and index) expressions.
enum AugTarget<'a> {
    Index { recv: &'a str, idx: &'a str },
    Attr { recv: &'a str, field: &'a str },
}

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
            Stmt::ConstDecl { name, expr, .. } => {
                let value = self.expr(expr, emit)?;
                out.push_str(&format!("{pad}{}\n", self.emit_bind(emit, name, &value)));
            }
            Stmt::Decl {
                names, rest, expr, ..
            } => {
                if names.len() == 1 && rest.is_none() {
                    let value = self.expr(expr, emit)?;
                    out.push_str(&format!(
                        "{pad}{}\n",
                        self.emit_bind(emit, &names[0], &value)
                    ));
                } else {
                    let src = self.expr(expr, emit)?;
                    let n = self.emit_unpack(emit, level, &src, names.len(), rest.is_some(), out);
                    for (i, name) in names.iter().enumerate() {
                        let value = format!("vals{n}[{i}].clone()");
                        out.push_str(&format!("{pad}{}\n", self.emit_bind(emit, name, &value)));
                    }
                    if let Some(rest) = rest {
                        let value = format!("vals{n}[{}].clone()", names.len());
                        out.push_str(&format!("{pad}{}\n", self.emit_bind(emit, rest, &value)));
                    }
                }
            }
            Stmt::Assign {
                targets,
                rest,
                expr,
                op,
                span,
                ..
            } => {
                // A destructuring assignment unpacks one right-hand value across
                // several targets; a plain or augmented assignment is single-target
                // (the parser guarantees an augmented op carries one target and no
                // collector).
                if op.is_none() && (targets.len() > 1 || rest.is_some()) {
                    let src = self.expr(expr, emit)?;
                    let n = self.emit_unpack(emit, level, &src, targets.len(), rest.is_some(), out);
                    for (i, target) in targets.iter().enumerate() {
                        let value = format!("vals{n}[{i}].clone()");
                        self.emit_store(emit, level, target, &value, *span, out)?;
                    }
                    if let Some(rest) = rest {
                        let value = format!("vals{n}[{}].clone()", targets.len());
                        self.emit_store(emit, level, rest, &value, *span, out)?;
                    }
                } else {
                    let target = &targets[0];
                    let rhs = self.expr(expr, emit)?;
                    match op {
                        None => self.emit_store(emit, level, target, &rhs, *span, out)?,
                        Some(op) => self.emit_aug_assign(emit, level, target, *op, &rhs, out)?,
                    }
                }
            }
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
                vars,
                rest,
                iter,
                body,
                ..
            } => {
                let iter_expr = self.expr(iter, emit)?;
                let iter_call = self.fail(emit, format!("iter_value(&{iter_expr})"));
                let label = emit.counter;
                emit.counter += 1;
                out.push_str(&format!("{pad}'l{label}: for item in {iter_call} {{\n"));
                emit.loop_stack.push(label);
                let inner = "    ".repeat(level + 1);
                if vars.len() == 1 && rest.is_none() {
                    out.push_str(&format!(
                        "{inner}{}\n",
                        self.emit_bind(emit, &vars[0], "item")
                    ));
                } else {
                    let n =
                        self.emit_unpack(emit, level + 1, "item", vars.len(), rest.is_some(), out);
                    for (i, var) in vars.iter().enumerate() {
                        let value = format!("vals{n}[{i}].clone()");
                        out.push_str(&format!("{inner}{}\n", self.emit_bind(emit, var, &value)));
                    }
                    if let Some(rest) = rest {
                        let value = format!("vals{n}[{}].clone()", vars.len());
                        out.push_str(&format!("{inner}{}\n", self.emit_bind(emit, rest, &value)));
                    }
                }
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
                let file_expr = if self.multifile {
                    "FILES[env.cur_file as usize].0".to_string()
                } else {
                    format!("\"{}\"", escape_str(&self.files[0].path))
                };
                let error_value = format!("error_value(&e, {file_expr}, env.cur_line)");
                out.push_str(&format!(
                    "{inner}{}\n",
                    self.emit_bind(emit, err_name, &error_value)
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
            Stmt::Amaze { cond, message, .. } => {
                let cond = self.expr(cond, emit)?;
                // The message lives inside the failure branch, so it is built only
                // when the assertion fails (matching Python's lazy assert message).
                let message = match message {
                    Some(message) => format!("Some(&{})", self.expr(message, emit)?),
                    None => "None".to_string(),
                };
                let raise = match emit.try_stack.last() {
                    Some(label) => format!("break 'p{label} Err(assert_error({message}))"),
                    None => format!("return Err(assert_error({message}))"),
                };
                out.push_str(&format!("{pad}if !({cond}).truthy() {{ {raise}; }}\n"));
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
            Stmt::Import { module, span, .. } => {
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

    /// Store an already-computed value into a plain (non-augmented) assignment
    /// target — a name, an item `xs[0]`, or a field `x.name`. Shared by
    /// single-target assignment and each target of a destructuring assignment.
    fn emit_store(
        &self,
        emit: &mut Emit,
        level: usize,
        target: &Expr,
        value: &str,
        span: Span,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let pad = "    ".repeat(level);
        match target {
            Expr::Ident { name, .. } => {
                self.check_writable(emit, name, span)?;
                out.push_str(&format!("{pad}{}\n", self.emit_bind(emit, name, value)));
            }
            Expr::Index { obj, index, .. } => {
                let recv = self.expr(obj, emit)?;
                let idx = self.expr(index, emit)?;
                let call = format!("index_set(&{recv}, &{idx}, {value})");
                out.push_str(&format!("{pad}{};\n", self.fail(emit, call)));
            }
            Expr::Attr {
                obj,
                name,
                span: attr_span,
                ..
            } => {
                self.reject_module_target(emit, obj, *attr_span)?;
                let recv = self.expr(obj, emit)?;
                let field = escape_str(name);
                let call = format!("attr_set(&{recv}, \"{field}\", {value})");
                out.push_str(&format!("{pad}{};\n", self.fail(emit, call)));
            }
            _ => unreachable!("compiler bug: parser guarantees a valid assign target"),
        }
        Ok(())
    }

    /// `target op= rhs` for a single target: an augmented assignment reads the
    /// current value, combines it with `rhs`, and stores the result back.
    fn emit_aug_assign(
        &self,
        emit: &mut Emit,
        level: usize,
        target: &Expr,
        op: BinOp,
        rhs: &str,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let pad = "    ".repeat(level);
        match target {
            Expr::Ident { name, .. } => {
                self.check_writable(emit, name, target.span())?;
                let cur = self.resolve_read(emit, name);
                let value = self.fail(emit, format!("{}({cur}, {rhs})", binop_call(op)));
                out.push_str(&format!("{pad}{}\n", self.emit_bind(emit, name, &value)));
            }
            Expr::Index { obj, index, .. } => {
                let recv = self.expr(obj, emit)?;
                let idx = self.expr(index, emit)?;
                let aug = AugTarget::Index {
                    recv: &recv,
                    idx: &idx,
                };
                self.emit_aug(emit, level, aug, op, rhs, out);
            }
            Expr::Attr {
                obj,
                name,
                span: attr_span,
                ..
            } => {
                self.reject_module_target(emit, obj, *attr_span)?;
                let recv = self.expr(obj, emit)?;
                let field = escape_str(name);
                let aug = AugTarget::Attr {
                    recv: &recv,
                    field: &field,
                };
                self.emit_aug(emit, level, aug, op, rhs, out);
            }
            _ => unreachable!("compiler bug: parser guarantees a valid assign target"),
        }
        Ok(())
    }

    /// A module's members are read-only, so `nerd.pi = …` (or `nerd.pi += …`) is
    /// rejected before emitting a store.
    fn reject_module_target(&self, emit: &Emit, obj: &Expr, span: Span) -> Result<(), Diagnostic> {
        if let Expr::Ident { name: base, .. } = obj {
            if emit.module(base).is_some() || emit.user_module(base).is_some() {
                return Err(self
                    .diag(span, "cannot assign into a module")
                    .with_headline("very module. much fixed.")
                    .with_hint("a module's members are read-only"));
            }
        }
        Ok(())
    }

    /// Emit the prologue of a destructuring bind: snapshot the right-hand value
    /// into `src{n}`, then split it into `vals{n}` — a `Vec<Value>` of `fixed`
    /// elements, plus a trailing collector List when `rest` is set. `unpack_value`
    /// is fallible (a non-iterable value or a length mismatch), so the failure is
    /// routed through `fail` for `pls`/`oh no` to catch. Returns the counter `n`
    /// the caller reads each `vals{n}[i]` from.
    fn emit_unpack(
        &self,
        emit: &mut Emit,
        level: usize,
        src: &str,
        fixed: usize,
        rest: bool,
        out: &mut String,
    ) -> u32 {
        let pad = "    ".repeat(level);
        let n = emit.counter;
        emit.counter += 1;
        out.push_str(&format!("{pad}let src{n} = {src};\n"));
        let call = format!("unpack_value(&src{n}, {fixed}, {rest})");
        let vals = self.fail(emit, call);
        out.push_str(&format!("{pad}let vals{n} = {vals};\n"));
        n
    }

    /// `target op= rhs` for an item or field target: bind the receiver (and index)
    /// to temporaries so each is evaluated once, then read the current value,
    /// combine it with `rhs`, and store the result back.
    fn emit_aug(
        &self,
        emit: &mut Emit,
        level: usize,
        target: AugTarget,
        op: BinOp,
        rhs: &str,
        out: &mut String,
    ) {
        let pad = "    ".repeat(level);
        let inner = "    ".repeat(level + 1);
        let n = emit.counter;
        emit.counter += 1;
        out.push_str(&format!("{pad}{{\n"));
        let (get, set) = match target {
            AugTarget::Index { recv, idx } => {
                out.push_str(&format!("{inner}let recv{n} = {recv};\n"));
                out.push_str(&format!("{inner}let idx{n} = {idx};\n"));
                (
                    format!("index_get(&recv{n}, &idx{n})"),
                    format!("index_set(&recv{n}, &idx{n}, new{n})"),
                )
            }
            AugTarget::Attr { recv, field } => {
                out.push_str(&format!("{inner}let recv{n} = {recv};\n"));
                (
                    format!("attr_get(&recv{n}, \"{field}\")"),
                    format!("attr_set(&recv{n}, \"{field}\", new{n})"),
                )
            }
        };
        let get = self.fail(emit, get);
        let combine = self.fail(emit, format!("{}(cur{n}, {rhs})", binop_call(op)));
        let set = self.fail(emit, set);
        out.push_str(&format!("{inner}let cur{n} = {get};\n"));
        out.push_str(&format!("{inner}let new{n} = {combine};\n"));
        out.push_str(&format!("{inner}{set};\n"));
        out.push_str(&format!("{pad}}}\n"));
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
