//! Calls in every form Doge has: direct function calls, calls through a function
//! value, method dispatch, constructors, module-member calls, and `super`. Each
//! resolves its target, evaluates arguments, binds them to the callee's header
//! (defaults, keyword arguments, and the `many` variadic), and runs the body.

use doge_compiler as dc;
use doge_runtime::{
    builtin_method, callee_function, enter_call, exit_call, function_arity_error, object_class_id,
    DogeError, DogeResult, Value,
};

use crate::natives::call_native;
use crate::{cell, scope, Callable, Flow, Interp, Scope, Template};

/// A call's evaluated arguments: positional values, then `(name, value)` keyword
/// pairs in source order.
type EvaluatedArgs = (Vec<Value>, Vec<(String, Value)>);

impl Interp {
    /// Evaluate a call expression, resolving the callee the same way the compiler
    /// does: a known function/constructor/module member statically, anything else
    /// through the value it evaluates to.
    pub(crate) fn eval_call(
        &mut self,
        callee: &dc::Expr,
        args: &[dc::Expr],
        kwargs: &[(String, dc::Expr)],
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Value> {
        match callee {
            dc::Expr::Ident { name, .. } => {
                // A bound name (local, or a top-level function/variable held as a
                // value) is called through its value.
                if let Some(cell) = self.lookup(frame, fid, name) {
                    let value = cell.borrow().clone();
                    let (args, kwargs) = self.eval_args(args, kwargs, frame, fid)?;
                    return self.call_value(value, args, kwargs);
                }
                if let Some(id) = self.builtin_ids.get(name).copied() {
                    let args = self.eval_positional(args, frame, fid)?;
                    return self.call_id(id, Vec::new(), args, Vec::new(), name);
                }
                if let Some(class_id) = self.class_id_in(fid, name) {
                    let (args, kwargs) = self.eval_args(args, kwargs, frame, fid)?;
                    return self.construct(class_id, args, kwargs, name);
                }
                Err(DogeError::type_error(format!(
                    "cannot call {name} — it is not a function"
                )))
            }
            // `nerd.sqrt(...)` / `utils.square(...)` — a member call on a module.
            dc::Expr::Attr { obj, name, .. }
                if matches!(obj.as_ref(), dc::Expr::Ident { name: base, .. }
                    if self.lookup(frame, fid, base).is_none()
                        && self.import_ref(fid, base).is_some()) =>
            {
                let dc::Expr::Ident { name: base, .. } = obj.as_ref() else {
                    unreachable!("guarded to an Ident base")
                };
                let module = self
                    .import_ref(fid, base)
                    .expect("guarded: base is an import");
                self.call_module_member(module, base, name, args, kwargs, frame, fid)
            }
            // `kabosu.speak(...)` — a method call, dispatched on the receiver.
            dc::Expr::Attr { obj, name, .. } => {
                let recv = self.eval(obj, frame, fid)?;
                let args = self.eval_positional(args, frame, fid)?;
                self.call_method(recv, name, args)
            }
            // Any other callee expression is called through its value.
            other => {
                let value = self.eval(other, frame, fid)?;
                let (args, kwargs) = self.eval_args(args, kwargs, frame, fid)?;
                self.call_value(value, args, kwargs)
            }
        }
    }

    /// Evaluate positional arguments only.
    fn eval_positional(
        &mut self,
        args: &[dc::Expr],
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for arg in args {
            out.push(self.eval(arg, frame, fid)?);
        }
        Ok(out)
    }

    /// Evaluate positional then keyword arguments, left to right.
    fn eval_args(
        &mut self,
        args: &[dc::Expr],
        kwargs: &[(String, dc::Expr)],
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<EvaluatedArgs> {
        let positional = self.eval_positional(args, frame, fid)?;
        let mut keyword = Vec::with_capacity(kwargs.len());
        for (name, value) in kwargs {
            keyword.push((name.clone(), self.eval(value, frame, fid)?));
        }
        Ok((positional, keyword))
    }

    /// Call a function value: unwrap it, then dispatch on its `fn_id`. Arity
    /// diagnostics name the function by its own definition name, not the call site.
    fn call_value(
        &mut self,
        value: Value,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
    ) -> DogeResult<Value> {
        let func = callee_function(&value)?;
        self.call_id(
            func.fn_id as usize,
            func.captures.clone(),
            args,
            kwargs,
            &func.name,
        )
    }

    /// Dispatch a call to the callable at `fn_id`: a native, or a user function
    /// with its captured cells.
    fn call_id(
        &mut self,
        id: usize,
        captures: Vec<crate::Cell>,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
        label: &str,
    ) -> DogeResult<Value> {
        let callable = self.callables[id].clone();
        match callable.as_ref() {
            Callable::Native(native) => call_native(native, args),
            Callable::User(template) => {
                self.call_user(template, &captures, args, kwargs, None, label)
            }
        }
    }

    /// Run a user function/method/closure body: guard recursion, build its frame
    /// (captures, `self`, parameters, hoisted locals), execute, and return the
    /// value it `return`s (or `none` if it falls off the end).
    fn call_user(
        &mut self,
        template: &Template,
        captures: &[crate::Cell],
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
        self_value: Option<Value>,
        label: &str,
    ) -> DogeResult<Value> {
        enter_call(&mut self.depth)?;
        let saved_class = self.current_method_class;
        self.current_method_class = template.method_class;
        let result = self.user_body(template, captures, args, kwargs, self_value, label);
        self.current_method_class = saved_class;
        exit_call(&mut self.depth);
        result
    }

    fn user_body(
        &mut self,
        template: &Template,
        captures: &[crate::Cell],
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
        self_value: Option<Value>,
        label: &str,
    ) -> DogeResult<Value> {
        let bound = self.bind_args(&template.params, args, kwargs, label, template.file_id)?;

        let frame = scope();
        {
            let mut f = frame.borrow_mut();
            for (name, captured) in template.capture_names.iter().zip(captures) {
                f.insert(name.clone(), captured.clone());
            }
            if let Some(self_value) = self_value {
                f.insert("self".to_string(), cell(self_value));
            }
            for (name, value) in template.params.binding_names().iter().zip(bound) {
                f.insert(name.clone(), cell(value));
            }
        }
        // Hoist the body's remaining local names to `none`, like the compiler's
        // hoisted `Env` fields; nested-function values bind when their statement runs.
        for name in dc::hoisted_names(&template.body) {
            frame
                .borrow_mut()
                .entry(name)
                .or_insert_with(|| cell(Value::None));
        }

        match self.exec_stmts(&template.body, &frame, template.file_id)? {
            Flow::Return(value) => Ok(value),
            // Falling off the end (or a checked-away stray bork/continue) yields none.
            _ => Ok(Value::None),
        }
    }

    /// Map a call's positional and keyword arguments onto a header's slots — filling
    /// from positionals, then keywords, then defaults, and collecting any surplus
    /// into the `many` variadic — returning the values in binding order. The arity
    /// and keyword diagnostics match the compiler's wording.
    fn bind_args(
        &mut self,
        params: &dc::Params,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
        label: &str,
        fid: u32,
    ) -> DogeResult<Vec<Value>> {
        let n = params.params.len();
        let has_vararg = params.has_vararg();
        let required = params.required();
        let max = params.max_positional();
        let total = args.len() + kwargs.len();

        if !has_vararg && args.len() > n {
            return Err(function_arity_error(label, required, max, total));
        }

        let mut slot: Vec<Option<Value>> = vec![None; n];
        let mut extras: Vec<Value> = Vec::new();
        for (i, arg) in args.into_iter().enumerate() {
            if i < n {
                slot[i] = Some(arg);
            } else {
                extras.push(arg);
            }
        }
        for (name, value) in kwargs {
            match params.params.iter().position(|p| p.name == name) {
                Some(idx) if slot[idx].is_none() => slot[idx] = Some(value),
                Some(_) => {
                    return Err(DogeError::type_error(format!(
                        "{label} got parameter {name} twice"
                    )))
                }
                None => {
                    return Err(DogeError::type_error(format!(
                        "{label} has no parameter {name}"
                    )))
                }
            }
        }

        let mut out = Vec::with_capacity(n + has_vararg as usize);
        for (i, filled) in slot.into_iter().enumerate() {
            match filled {
                Some(value) => out.push(value),
                None => match &params.params[i].default {
                    Some(default) => out.push(self.eval(default, &scope(), fid)?),
                    None => return Err(function_arity_error(label, required, max, total)),
                },
            }
        }
        if has_vararg {
            out.push(Value::list(extras));
        }
        Ok(out)
    }

    /// Dispatch a method call: an object routes through its class's method table
    /// (walking its ancestry); any other receiver forwards to the runtime's
    /// collection methods, exactly like the compiled dispatcher.
    fn call_method(&mut self, recv: Value, name: &str, args: Vec<Value>) -> DogeResult<Value> {
        if !matches!(recv, Value::Object(_)) {
            return builtin_method(&recv, name, args);
        }
        let class_id = object_class_id(&recv)?;
        match self.resolve_method(class_id, name) {
            Some((fn_id, def_class)) => {
                let label = format!("{}.{name}", self.classes[def_class as usize].name);
                self.invoke_method(fn_id, recv, args, &label)
            }
            None => Err(doge_runtime::no_such_method(&recv, name)),
        }
    }

    /// Invoke a resolved method template with `recv` bound as `self`.
    fn invoke_method(
        &mut self,
        fn_id: usize,
        recv: Value,
        args: Vec<Value>,
        label: &str,
    ) -> DogeResult<Value> {
        let callable = self.callables[fn_id].clone();
        let Callable::User(template) = callable.as_ref() else {
            unreachable!("interp bug: a method id points at a native");
        };
        self.call_user(template, &[], args, Vec::new(), Some(recv), label)
    }

    /// `super.method(args)`: resolve `method` in the enclosing class's parent chain
    /// and call it with the current `self`.
    pub(crate) fn eval_super_call(
        &mut self,
        method: &str,
        args: &[dc::Expr],
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Value> {
        let class_id = self
            .current_method_class
            .expect("checker guarantees super is inside a method");
        let parent = self.classes[class_id as usize]
            .parent
            .expect("checker guarantees the class has a parent");
        let (fn_id, def_class) = self
            .resolve_method(parent, method)
            .expect("checker guarantees a parent defines the method");
        let self_value = self
            .lookup(frame, fid, "self")
            .expect("a method frame always binds self")
            .borrow()
            .clone();
        let args = self.eval_positional(args, frame, fid)?;
        let label = format!("{}.{method}", self.classes[def_class as usize].name);
        self.invoke_method(fn_id, self_value, args, &label)
    }

    /// The method named `name` callable on `class_id`: the nearest definition up
    /// its ancestry, returning its `fn_id` and the class that defines it.
    fn resolve_method(&self, class_id: u32, name: &str) -> Option<(usize, u32)> {
        let mut current = Some(class_id);
        let mut guard = 0;
        while let Some(cid) = current {
            let class = &self.classes[cid as usize];
            if let Some(fn_id) = class.methods.get(name) {
                return Some((*fn_id, cid));
            }
            guard += 1;
            if guard > self.classes.len() {
                break;
            }
            current = class.parent;
        }
        None
    }

    /// Construct an instance of `class_id`: build the object, then run its
    /// effective `init` (its own or the nearest inherited one) with `self` bound.
    fn construct(
        &mut self,
        class_id: u32,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
        label: &str,
    ) -> DogeResult<Value> {
        let class = self.classes[class_id as usize].clone();
        let object = Value::object(class_id, &class.name);
        match self.resolve_method(class_id, "init") {
            Some((fn_id, _)) => {
                self.invoke_method_kw(fn_id, object.clone(), args, kwargs, label)?;
            }
            None => {
                // No init anywhere in the chain: construction takes no arguments.
                self.bind_args(&dc::Params::default(), args, kwargs, label, class.file_id)?;
            }
        }
        Ok(object)
    }

    /// Invoke a method template that accepts keyword arguments (a constructor's
    /// `init`), binding `recv` as `self`.
    fn invoke_method_kw(
        &mut self,
        fn_id: usize,
        recv: Value,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
        label: &str,
    ) -> DogeResult<Value> {
        let callable = self.callables[fn_id].clone();
        let Callable::User(template) = callable.as_ref() else {
            unreachable!("interp bug: a method id points at a native");
        };
        self.call_user(template, &[], args, kwargs, Some(recv), label)
    }

    /// A call to a module member: a stdlib function, or a user module's function or
    /// constructor.
    #[allow(clippy::too_many_arguments)]
    fn call_module_member(
        &mut self,
        module: crate::ModuleRef,
        base: &str,
        member: &str,
        args: &[dc::Expr],
        kwargs: &[(String, dc::Expr)],
        frame: &Scope,
        fid: u32,
    ) -> DogeResult<Value> {
        match module {
            crate::ModuleRef::Stdlib(m) => {
                let args = self.eval_positional(args, frame, fid)?;
                match self
                    .module_fn_ids
                    .get(&(m.name.to_string(), member.to_string()))
                {
                    Some(id) => self.call_id(*id, Vec::new(), args, Vec::new(), member),
                    None => Err(DogeError::attr_error(format!(
                        "{base} has no member {member}"
                    ))),
                }
            }
            crate::ModuleRef::User(mfid) => {
                let (args, kwargs) = self.eval_args(args, kwargs, frame, fid)?;
                if let Some(class_id) = self.class_id_in(mfid, member) {
                    let label = format!("{base}.{member}");
                    return self.construct(class_id, args, kwargs, &label);
                }
                let value = self
                    .lookup(&self.globals(mfid), mfid, member)
                    .map(|c| c.borrow().clone())
                    .ok_or_else(|| {
                        DogeError::attr_error(format!("{base} has no member {member}"))
                    })?;
                self.call_value(value, args, kwargs)
            }
        }
    }
}
