//! Statement execution. Each statement runs against a call frame (`frame`) and
//! the file it belongs to (`fid`); control flow that escapes a statement —
//! `return`, `bork`, `continue` — bubbles up as a [`Flow`] the caller acts on.

use doge_compiler as dc;
use doge_runtime::{
    assert_error, bonk_error, error_value, index_set, iter_value, unpack_value, DogeResult, Value,
};

use crate::{cell, Flow, Interp, Scope};

impl Interp {
    /// Run a block of statements, stopping early if one escapes with a non-`Normal`
    /// flow (which the caller propagates or acts on).
    pub(crate) fn exec_stmts(
        &mut self,
        stmts: &[dc::Stmt],
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Flow> {
        for stmt in stmts {
            match self.exec_stmt(stmt, frame, fid)? {
                Flow::Normal => {}
                escaped => return Ok(escaped),
            }
        }
        Ok(Flow::Normal)
    }

    pub(crate) fn exec_stmt(
        &mut self,
        stmt: &dc::Stmt,
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Flow> {
        self.mark(fid, stmt.span());
        match stmt {
            dc::Stmt::Decl {
                names, rest, expr, ..
            } => {
                let value = self.eval(expr, frame, fid)?;
                self.bind_destructure(names, rest.as_deref(), value, frame, fid)?;
                Ok(Flow::Normal)
            }
            dc::Stmt::ConstDecl { name, expr, .. } => {
                let value = self.eval(expr, frame, fid)?;
                self.bind_name(frame, name, value);
                Ok(Flow::Normal)
            }
            // Imports are resolved during integration; nothing to run.
            dc::Stmt::Import { .. } => Ok(Flow::Normal),
            dc::Stmt::Assign {
                targets,
                rest,
                expr,
                op,
                ..
            } => {
                self.exec_assign(targets, rest.as_ref(), expr, *op, frame, fid)?;
                Ok(Flow::Normal)
            }
            dc::Stmt::Bark { expr, .. } => {
                let value = self.eval(expr, frame, fid)?;
                doge_runtime::bark(&value);
                Ok(Flow::Normal)
            }
            dc::Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (cond, body) in branches {
                    if self.eval(cond, frame, fid)?.truthy() {
                        return self.exec_stmts(body, frame, fid);
                    }
                }
                if let Some(body) = else_body {
                    return self.exec_stmts(body, frame, fid);
                }
                Ok(Flow::Normal)
            }
            dc::Stmt::For {
                vars,
                rest,
                iter,
                body,
                ..
            } => self.exec_for(vars, rest.as_deref(), iter, body, frame, fid),
            dc::Stmt::While { cond, body, .. } => {
                loop {
                    self.mark(fid, cond.span());
                    if !self.eval(cond, frame, fid)?.truthy() {
                        break;
                    }
                    match self.exec_stmts(body, frame, fid)? {
                        Flow::Normal | Flow::Continue => {}
                        Flow::Break => break,
                        ret @ Flow::Return(_) => return Ok(ret),
                    }
                }
                Ok(Flow::Normal)
            }
            dc::Stmt::FuncDef { name, span, .. } => {
                let value = self.make_function(*span, name, frame, fid);
                self.bind_name(frame, name, value);
                Ok(Flow::Normal)
            }
            // Classes are registered during analysis; only top-level object
            // definitions are supported, matching the compiler.
            dc::Stmt::ObjDef { .. } => Ok(Flow::Normal),
            dc::Stmt::Try {
                body,
                err_name,
                handler,
                ..
            } => match self.exec_stmts(body, frame, fid) {
                Ok(flow) => Ok(flow),
                Err(err) => {
                    let value = error_value(&err, &self.cur_path(), self.cur_line);
                    self.bind_name(frame, err_name, value);
                    self.exec_stmts(handler, frame, fid)
                }
            },
            dc::Stmt::Return { expr, .. } => {
                let value = match expr {
                    Some(expr) => self.eval(expr, frame, fid)?,
                    None => Value::None,
                };
                Ok(Flow::Return(value))
            }
            dc::Stmt::Bonk { expr, .. } => {
                let value = self.eval(expr, frame, fid)?;
                Err(bonk_error(&value))
            }
            dc::Stmt::Amaze { cond, message, .. } => {
                if self.eval(cond, frame, fid)?.truthy() {
                    return Ok(Flow::Normal);
                }
                // The message is evaluated only on failure, mirroring the compiler.
                let message = match message {
                    Some(message) => Some(self.eval(message, frame, fid)?),
                    None => None,
                };
                Err(assert_error(message.as_ref()))
            }
            dc::Stmt::Bork { .. } => Ok(Flow::Break),
            dc::Stmt::Continue { .. } => Ok(Flow::Continue),
            dc::Stmt::ExprStmt { expr } => {
                self.eval(expr, frame, fid)?;
                Ok(Flow::Normal)
            }
        }
    }

    /// A `for` loop: snapshot the iterable, then bind each element (destructuring
    /// when the header names several targets) and run the body, honoring `bork`
    /// and `continue`.
    fn exec_for(
        &mut self,
        vars: &[String],
        rest: Option<&str>,
        iter: &dc::Expr,
        body: &[dc::Stmt],
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Flow> {
        let iterable = self.eval(iter, frame, fid)?;
        for item in iter_value(&iterable)? {
            self.bind_destructure(vars, rest, item, frame, fid)?;
            match self.exec_stmts(body, frame, fid)? {
                Flow::Normal | Flow::Continue => {}
                Flow::Break => break,
                ret @ Flow::Return(_) => return Ok(ret),
            }
        }
        Ok(Flow::Normal)
    }

    /// Bind `value` to one or more names. A single name with no collector binds
    /// directly; anything else unpacks the value across the names plus an optional
    /// trailing `many` collector, exactly like the compiler's destructuring.
    fn bind_destructure(
        &mut self,
        names: &[String],
        rest: Option<&str>,
        value: Value,
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<()> {
        if names.len() == 1 && rest.is_none() {
            self.bind_name(frame, &names[0], value);
            return Ok(());
        }
        let mut values = unpack_value(&value, names.len(), rest.is_some())?;
        if let Some(rest) = rest {
            let collector = values
                .pop()
                .expect("unpack_value returns the collector last");
            self.bind_name(frame, rest, collector);
        }
        for (name, value) in names.iter().zip(values) {
            self.bind_name(frame, name, value);
        }
        let _ = fid;
        Ok(())
    }

    /// Execute an assignment: a plain, augmented, or destructuring store into one
    /// or more targets (`name`, `xs[i]`, `x.field`).
    fn exec_assign(
        &mut self,
        targets: &[dc::Expr],
        rest: Option<&dc::Expr>,
        expr: &dc::Expr,
        op: Option<dc::BinOp>,
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<()> {
        if let Some(op) = op {
            // Augmented assignment is single-target (parser-guaranteed).
            return self.exec_augmented(&targets[0], op, expr, frame, fid);
        }
        let value = self.eval(expr, frame, fid)?;
        if targets.len() == 1 && rest.is_none() {
            return self.assign_target(&targets[0], value, frame, fid);
        }
        let mut values = unpack_value(&value, targets.len(), rest.is_some())?;
        if let Some(rest) = rest {
            let collector = values
                .pop()
                .expect("unpack_value returns the collector last");
            self.assign_target(rest, collector, frame, fid)?;
        }
        for (target, value) in targets.iter().zip(values) {
            self.assign_target(target, value, frame, fid)?;
        }
        Ok(())
    }

    /// `target op= rhs`: read the target once, combine it with the right-hand side,
    /// and store the result back. Index/field receivers are evaluated once.
    fn exec_augmented(
        &mut self,
        target: &dc::Expr,
        op: dc::BinOp,
        rhs: &dc::Expr,
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<()> {
        let rhs = self.eval(rhs, frame, fid)?;
        match target {
            dc::Expr::Ident { name, .. } => {
                let current = self.read_name(frame, fid, name)?;
                let updated = self.binop(op, current, rhs)?;
                self.store_name(frame, fid, name, updated);
                Ok(())
            }
            dc::Expr::Index { obj, index, .. } => {
                let recv = self.eval(obj, frame, fid)?;
                let idx = self.eval(index, frame, fid)?;
                let current = doge_runtime::index_get(&recv, &idx)?;
                let updated = self.binop(op, current, rhs)?;
                index_set(&recv, &idx, updated)
            }
            dc::Expr::Attr { obj, name, .. } => {
                let recv = self.eval(obj, frame, fid)?;
                let current = doge_runtime::attr_get(&recv, name)?;
                let updated = self.binop(op, current, rhs)?;
                doge_runtime::attr_set(&recv, name, updated)
            }
            // The parser guarantees a valid augmented target.
            _ => Ok(()),
        }
    }

    /// Store `value` into an assignment target (`name`, `xs[i]`, or `x.field`).
    fn assign_target(
        &mut self,
        target: &dc::Expr,
        value: Value,
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<()> {
        match target {
            dc::Expr::Ident { name, .. } => {
                self.store_name(frame, fid, name, value);
                Ok(())
            }
            dc::Expr::Index { obj, index, .. } => {
                let recv = self.eval(obj, frame, fid)?;
                let idx = self.eval(index, frame, fid)?;
                index_set(&recv, &idx, value)
            }
            dc::Expr::Attr { obj, name, .. } => {
                let recv = self.eval(obj, frame, fid)?;
                doge_runtime::attr_set(&recv, name, value)
            }
            _ => Ok(()),
        }
    }

    /// Declare or overwrite `name` in `frame`, reusing its existing cell so a
    /// closure that captured it keeps seeing updates.
    pub(crate) fn bind_name(&self, frame: &Scope, name: &str, value: Value) {
        if let Some(existing) = frame.borrow().get(name) {
            *existing.borrow_mut() = value;
            return;
        }
        frame.borrow_mut().insert(name.to_string(), cell(value));
    }

    /// Reassign an already-declared `name`, updating whichever scope holds it.
    fn store_name(&self, frame: &Scope, fid: u32, name: &str, value: Value) {
        if let Some(existing) = self.lookup(frame, fid, name) {
            *existing.borrow_mut() = value;
        } else {
            frame.borrow_mut().insert(name.to_string(), cell(value));
        }
    }
}
