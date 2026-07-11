use super::*;

impl Codegen {
    /// Emit a top-level function as a wrapper + body pair, mangled by its file id.
    pub(super) fn function(
        &self,
        span: Span,
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let info = self.fn_info(emit, span);
        self.emit_callable(
            &func_wrapper(emit.file_id, &info.name),
            &func_body(emit.file_id, &info.name),
            &[],
            &info.params,
            &info.body,
            &info.cell_names,
            emit,
            out,
        )
    }

    /// Emit an object method as an `mf_`/`mb_` pair. A method is an ordinary
    /// callable whose first parameter is the implicit `self` receiver.
    pub(super) fn method(
        &self,
        class: &Class,
        span: Span,
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let info = self.fn_info(emit, span);
        // `params` already carries `self` first (added during analysis).
        self.emit_callable(
            &format!("{METHOD_PREFIX}{}_{}", class.id, info.name),
            &format!("{METHOD_BODY_PREFIX}{}_{}", class.id, info.name),
            &[],
            &info.params,
            &info.body,
            &info.cell_names,
            emit,
            out,
        )
    }

    /// Emit a nested function as a `c_`/`cb_` pair, taking its captured cells as
    /// leading parameters.
    pub(super) fn closure(
        &self,
        info: &FnInfo,
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let id = info.fn_id.expect("compiler bug: closure without an id");
        self.emit_callable(
            &format!("{CLOSURE_PREFIX}{id}"),
            &format!("{CLOSURE_BODY_PREFIX}{id}"),
            &info.captures,
            &info.params,
            &info.body,
            &info.cell_names,
            emit,
            out,
        )
    }

    /// Emit a wrapper + body pair. The wrapper counts the call against the
    /// recursion limit and undoes it on every exit path — even a `?` inside the
    /// body — because `exit_call` runs after the body returns. Captured cells lead
    /// the parameter list; any local a nested closure captures becomes a `Cell`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_callable(
        &self,
        wrapper_name: &str,
        body_name: &str,
        captures: &[String],
        params: &[String],
        body: &[Stmt],
        cell_names: &HashSet<String>,
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let wrapper_params = signature(captures, params, false);
        out.push_str(&format!(
            "\nfn {wrapper_name}({wrapper_params}) -> DogeResult<Value> {{\n"
        ));
        out.push_str("    enter_call(&mut env.depth)?;\n");
        let call_args = {
            let mut v: Vec<String> = captures
                .iter()
                .chain(params.iter())
                .map(|p| format!("{NAME_PREFIX}{p}"))
                .collect();
            v.push("env".to_string());
            v.join(", ")
        };
        out.push_str(&format!("    let result = {body_name}({call_args});\n"));
        out.push_str("    exit_call(&mut env.depth);\n");
        out.push_str("    result\n");
        out.push_str("}\n");

        let body_params = signature(captures, params, true);
        out.push_str(&format!(
            "\nfn {body_name}({body_params}) -> DogeResult<Value> {{\n"
        ));

        let mut locals: HashMap<String, Local> = HashMap::new();
        // Captured cells arrive already shared, as `Cell` parameters.
        for name in captures {
            locals.insert(name.clone(), Local::Cell);
        }
        // A value parameter a nested closure captures is rebound to a fresh cell.
        for param in params {
            if cell_names.contains(param) {
                out.push_str(&format!(
                    "    let {NAME_PREFIX}{param} = Rc::new(RefCell::new({NAME_PREFIX}{param}));\n"
                ));
                locals.insert(param.clone(), Local::Cell);
            } else {
                locals.insert(param.clone(), Local::Plain);
            }
        }
        // Body-hoisted names (including nested-function names) get a fresh binding:
        // a `Cell` when captured or a function name, otherwise a plain `Value`.
        for name in hoisted_names(body) {
            if params.iter().any(|p| p == &name) {
                continue;
            }
            if cell_names.contains(&name) {
                out.push_str(&format!(
                    "    let {NAME_PREFIX}{name}: Cell = Rc::new(RefCell::new(Value::None));\n"
                ));
                locals.insert(name, Local::Cell);
            } else {
                out.push_str(&format!(
                    "    let mut {NAME_PREFIX}{name}: Value = Value::None;\n"
                ));
                locals.insert(name, Local::Plain);
            }
        }

        emit.locals = locals;
        emit.local_funcs = child_funcdefs(body)
            .into_iter()
            .map(|(name, params, _, _)| (name.to_string(), params.to_vec()))
            .collect();
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

    /// Emit a constructor `n_<id>`: build a fresh instance, run `init` (if the
    /// class has one), and return the object. The callsite wraps the `n_` call in
    /// the fail suffix, so the `?` on `init` here is correct.
    pub(super) fn constructor(&self, class: &Class, out: &mut String) {
        let init_params = class.init_params();
        let ctor_params = signature(&[], init_params, false);
        out.push_str(&format!(
            "\nfn {CTOR_PREFIX}{}({ctor_params}) -> DogeResult<Value> {{\n",
            class.id
        ));
        out.push_str(&format!(
            "    let obj = Value::object({}u32, \"{}\");\n",
            class.id,
            escape_str(&class.name)
        ));
        if class.methods.iter().any(|(name, _)| name == "init") {
            let mut args: Vec<String> = vec!["obj.clone()".to_string()];
            args.extend(init_params.iter().map(|p| format!("{NAME_PREFIX}{p}")));
            args.push("env".to_string());
            out.push_str(&format!(
                "    {METHOD_PREFIX}{}_init({})?;\n",
                class.id,
                args.join(", ")
            ));
        }
        out.push_str("    Ok(obj)\n");
        out.push_str("}\n");
    }
}
