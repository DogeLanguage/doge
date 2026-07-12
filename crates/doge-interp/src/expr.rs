//! Expression evaluation. Every operator and access routes to the same
//! `doge-runtime` function the generated Rust would call, so results match the
//! compiled program exactly.

use doge_compiler as dc;
use doge_runtime::{
    add, attr_get_or_bind, bitand, bitnot, bitor, bitxor, div, eq, floordiv, ge, gt, in_,
    index_get, interp, le, lt, mul, ne, neg, not_, not_in, pow, rem, shl, shr, slice_get, sub,
    DogeError, DogeResult, Value,
};

use crate::Interp;
use crate::{natives, ModuleRef, Scope};

impl Interp {
    /// Evaluate an expression to a value in the given call frame and file.
    pub(crate) fn eval(&mut self, expr: &dc::Expr, frame: &Scope, fid: u32) -> DogeResult<Value> {
        match expr {
            dc::Expr::Int { value, .. } => Ok(Value::Int(*value)),
            dc::Expr::Float { value, .. } => Ok(Value::Float(*value)),
            dc::Expr::Str { value, .. } => Ok(Value::str(value)),
            dc::Expr::Bool { value, .. } => Ok(Value::Bool(*value)),
            dc::Expr::None { .. } => Ok(Value::None),
            dc::Expr::Ident { name, .. } => self.eval_ident(name, frame, fid),
            dc::Expr::List { items, .. } => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(item, frame, fid)?);
                }
                Ok(Value::list(values))
            }
            dc::Expr::Dict { entries, .. } => {
                let mut pairs = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    let key = self.eval(key, frame, fid)?;
                    let value = self.eval(value, frame, fid)?;
                    pairs.push((key, value));
                }
                Value::dict_from_pairs(pairs)
            }
            dc::Expr::Binary { op, lhs, rhs, .. } => self.eval_binary(*op, lhs, rhs, frame, fid),
            dc::Expr::Unary { op, operand, .. } => {
                let value = self.eval(operand, frame, fid)?;
                match op {
                    dc::UnOp::Not => not_(value),
                    dc::UnOp::Neg => neg(value),
                    dc::UnOp::BitNot => bitnot(value),
                }
            }
            dc::Expr::Call {
                callee,
                args,
                kwargs,
                ..
            } => self.eval_call(callee, args, kwargs, frame, fid),
            dc::Expr::Index { obj, index, .. } => {
                let recv = self.eval(obj, frame, fid)?;
                let idx = self.eval(index, frame, fid)?;
                index_get(&recv, &idx)
            }
            dc::Expr::Slice {
                obj,
                start,
                end,
                step,
                ..
            } => {
                let recv = self.eval(obj, frame, fid)?;
                let start = self.eval_opt(start, frame, fid)?;
                let end = self.eval_opt(end, frame, fid)?;
                let step = self.eval_opt(step, frame, fid)?;
                slice_get(&recv, &start, &end, &step)
            }
            dc::Expr::Ternary {
                cond,
                then,
                otherwise,
                ..
            } => {
                if self.eval(cond, frame, fid)?.truthy() {
                    self.eval(then, frame, fid)
                } else {
                    self.eval(otherwise, frame, fid)
                }
            }
            dc::Expr::Attr { obj, name, .. } => self.eval_attr(obj, name, frame, fid),
            dc::Expr::SuperCall { method, args, .. } => {
                self.eval_super_call(method, args, frame, fid)
            }
            dc::Expr::StrInterp { parts, .. } => {
                let mut rendered = Vec::with_capacity(parts.len());
                for part in parts {
                    match part {
                        dc::InterpPart::Lit(text) => rendered.push(Value::str(text)),
                        dc::InterpPart::Expr(hole) => rendered.push(self.eval(hole, frame, fid)?),
                    }
                }
                Ok(interp(&rendered))
            }
        }
    }

    /// An optional slice bound: an omitted bound reads as `none`, matching the
    /// runtime's slice semantics.
    fn eval_opt(
        &mut self,
        expr: &Option<Box<dc::Expr>>,
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Value> {
        match expr {
            Some(expr) => self.eval(expr, frame, fid),
            None => Ok(Value::None),
        }
    }

    /// Resolve a bare name to a value: a variable/function binding, or a builtin
    /// used as a first-class function value.
    fn eval_ident(&self, name: &str, frame: &Scope, fid: u32) -> DogeResult<Value> {
        if let Some(cell) = self.lookup(frame, fid, name) {
            return Ok(cell.borrow().clone());
        }
        if let Some(id) = self.builtin_ids.get(name) {
            return Ok(Value::function(*id as u32, name, Vec::new()));
        }
        // A class name as a value: a callable that builds an instance, carrying its
        // constructor's `fn_id` so a call dispatches to `construct`.
        if let Some(cid) = self.class_id_in(fid, name) {
            let ctor = self.classes[cid as usize].ctor_fn_id;
            return Ok(Value::class(ctor as u32, name));
        }
        // A module name used as a value is rejected by the compiler, so a checked,
        // runnable program never reaches here; report it without panicking.
        Err(DogeError::type_error(format!(
            "cannot use {name} as a value"
        )))
    }

    /// Read the current value of an already-declared variable (an augmented
    /// assignment target).
    pub(crate) fn read_name(&self, frame: &Scope, fid: u32, name: &str) -> DogeResult<Value> {
        self.lookup(frame, fid, name)
            .map(|c| c.borrow().clone())
            .ok_or_else(|| DogeError::type_error(format!("doge does not know the name {name}")))
    }

    /// `a and b` / `a or b` short-circuit and yield a Bool, matching the compiler;
    /// every other binary operator maps to its runtime function.
    fn eval_binary(
        &mut self,
        op: dc::BinOp,
        lhs: &dc::Expr,
        rhs: &dc::Expr,
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Value> {
        match op {
            dc::BinOp::And => {
                let left = self.eval(lhs, frame, fid)?;
                if !left.truthy() {
                    Ok(Value::Bool(false))
                } else {
                    Ok(Value::Bool(self.eval(rhs, frame, fid)?.truthy()))
                }
            }
            dc::BinOp::Or => {
                let left = self.eval(lhs, frame, fid)?;
                if left.truthy() {
                    Ok(Value::Bool(true))
                } else {
                    Ok(Value::Bool(self.eval(rhs, frame, fid)?.truthy()))
                }
            }
            _ => {
                let left = self.eval(lhs, frame, fid)?;
                let right = self.eval(rhs, frame, fid)?;
                self.binop(op, left, right)
            }
        }
    }

    /// Apply a non-short-circuit binary operator by dispatching to its runtime
    /// function. Shared with augmented assignment.
    pub(crate) fn binop(&self, op: dc::BinOp, l: Value, r: Value) -> DogeResult<Value> {
        match op {
            dc::BinOp::Add => add(l, r),
            dc::BinOp::Sub => sub(l, r),
            dc::BinOp::Mul => mul(l, r),
            dc::BinOp::Div => div(l, r),
            dc::BinOp::FloorDiv => floordiv(l, r),
            dc::BinOp::Rem => rem(l, r),
            dc::BinOp::Pow => pow(l, r),
            dc::BinOp::BitAnd => bitand(l, r),
            dc::BinOp::BitOr => bitor(l, r),
            dc::BinOp::BitXor => bitxor(l, r),
            dc::BinOp::Shl => shl(l, r),
            dc::BinOp::Shr => shr(l, r),
            dc::BinOp::Eq => eq(l, r),
            dc::BinOp::NotEq => ne(l, r),
            dc::BinOp::Lt => lt(l, r),
            dc::BinOp::LtEq => le(l, r),
            dc::BinOp::Gt => gt(l, r),
            dc::BinOp::GtEq => ge(l, r),
            dc::BinOp::In => in_(l, r),
            dc::BinOp::NotIn => not_in(l, r),
            // Short-circuit operators are handled in `eval_binary`.
            dc::BinOp::And | dc::BinOp::Or => Ok(Value::Bool(l.truthy() && r.truthy())),
        }
    }

    /// `obj.name`: a stdlib constant or module member value when `obj` is an
    /// imported module name; otherwise a field/method read on the value.
    fn eval_attr(
        &mut self,
        obj: &dc::Expr,
        name: &str,
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Value> {
        if let dc::Expr::Ident { name: base, .. } = obj {
            // A local of the same name shadows the module (locals always win).
            if self.lookup(frame, fid, base).is_none() {
                if let Some(module) = self.import_ref(fid, base) {
                    return self.eval_module_member(module, base, name);
                }
            }
        }
        let recv = self.eval(obj, frame, fid)?;
        // A bare `obj.name` read binds a method when there is no such field —
        // the same rule the compiled `attr_get_or_bind` follows.
        attr_get_or_bind(&recv, name, &|cid, n| self.resolve_method(cid, n).is_some())
    }

    /// A module member read as a value: a stdlib constant or function, or a user
    /// module's constant or function.
    fn eval_module_member(&self, module: ModuleRef, base: &str, member: &str) -> DogeResult<Value> {
        match module {
            ModuleRef::Stdlib(m) => {
                if let Some(value) = natives::module_const(m.name, member) {
                    return Ok(value);
                }
                if let Some(id) = self
                    .module_fn_ids
                    .get(&(m.name.to_string(), member.to_string()))
                {
                    return Ok(Value::function(
                        *id as u32,
                        &format!("{}.{member}", m.name),
                        Vec::new(),
                    ));
                }
                Err(DogeError::attr_error(format!(
                    "{base} has no member {member}"
                )))
            }
            ModuleRef::User(mfid) => {
                // `utils.Shibe` as a value: the module's class as a callable, named
                // for the qualified path but sharing the class's constructor id.
                if let Some(cid) = self.class_id_in(mfid, member) {
                    let ctor = self.classes[cid as usize].ctor_fn_id;
                    return Ok(Value::class(ctor as u32, &format!("{base}.{member}")));
                }
                self.lookup(&self.globals(mfid), mfid, member)
                    .map(|c| c.borrow().clone())
                    .ok_or_else(|| DogeError::attr_error(format!("{base} has no member {member}")))
            }
        }
    }
}
