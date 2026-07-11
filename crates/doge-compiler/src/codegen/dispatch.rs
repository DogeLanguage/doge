use super::*;

impl Codegen {
    /// Emit the single `call_method` dispatcher: one arm per (class, method),
    /// each checking arity at runtime before calling the method wrapper. A
    /// non-`Object` receiver (a List or Dict method, or a method on any other
    /// value) is forwarded to the runtime's `builtin_method`. Emitted only when
    /// the script defines an object or calls a method somewhere.
    pub(super) fn dispatcher(&self, classes: &[Class], uses_method_call: bool) -> String {
        if classes.is_empty() && !uses_method_call {
            return String::new();
        }
        let mut out = String::new();
        out.push_str(
            "\nfn call_method(recv: Value, name: &str, mut args: Vec<Value>, env: &mut Env) -> DogeResult<Value> {\n",
        );
        out.push_str(
            "    if !matches!(recv, Value::Object(_)) { return builtin_method(&recv, name, args); }\n",
        );
        out.push_str("    match (object_class_id(&recv, name)?, name) {\n");
        for class in classes {
            for (method, params) in &class.methods {
                let arity = params.len();
                out.push_str(&format!(
                    "        ({}u32, \"{}\") => {{\n",
                    class.id,
                    escape_str(method)
                ));
                out.push_str(&format!(
                    "            if args.len() != {arity} {{ return Err(method_arity_error(\"{}\", \"{}\", {arity}, args.len())); }}\n",
                    escape_str(&class.name),
                    escape_str(method)
                ));
                let mut call_args = vec!["recv".to_string()];
                for _ in 0..arity {
                    call_args.push("args.remove(0)".to_string());
                }
                call_args.push("env".to_string());
                out.push_str(&format!(
                    "            {METHOD_PREFIX}{}_{method}({})\n",
                    class.id,
                    call_args.join(", ")
                ));
                out.push_str("        }\n");
            }
        }
        out.push_str("        _ => Err(no_such_method(&recv, name)),\n");
        out.push_str("    }\n");
        out.push_str("}\n");
        out
    }

    /// The `call_function` dispatcher: one arm per materialized `fn_id`, each
    /// checking arity before calling the target — a user function's
    /// recursion-guarded wrapper, or a builtin/module function directly. Emitted
    /// only when the script calls something through a value.
    pub(super) fn function_dispatcher(&self, emit: &Emit) -> String {
        if !emit.uses_call_function.get() {
            return String::new();
        }
        let mut ids: Vec<u32> = emit.materialized.borrow().iter().copied().collect();
        ids.sort_unstable();

        let mut out = String::new();
        out.push_str(
            "\nfn call_function(f: &FunctionData, mut args: Vec<Value>, env: &mut Env) -> DogeResult<Value> {\n",
        );
        out.push_str("    match f.fn_id {\n");
        for id in ids {
            let arm = &emit.analysis.registry[id as usize];
            out.push_str(&self.function_arm(id, arm));
        }
        // Unreachable for any value the runtime built, since `callee_function`
        // rejects non-functions before dispatch — but keep it non-panicking.
        out.push_str(
            "        _ => Err(DogeError::type_error(\"very confuse. much function.\")),\n",
        );
        out.push_str("    }\n");
        out.push_str("}\n");
        out
    }

    /// One arm of the `call_function` dispatcher for a given `fn_id`.
    pub(super) fn function_arm(&self, id: u32, arm: &ArmSpec) -> String {
        let mut out = String::new();
        out.push_str(&format!("        {id}u32 => {{\n"));
        match arm {
            ArmSpec::TopFunc {
                file_id,
                name,
                arity,
            } => {
                out.push_str(&Self::arity_guard(name, *arity));
                let mut call_args: Vec<String> =
                    (0..*arity).map(|_| "args.remove(0)".into()).collect();
                call_args.push("&mut *env".into());
                out.push_str(&format!(
                    "            {}({})\n",
                    func_wrapper(*file_id, name),
                    call_args.join(", ")
                ));
            }
            ArmSpec::Closure {
                name,
                id,
                arity,
                captures,
            } => {
                out.push_str(&Self::arity_guard(name, *arity));
                let mut call_args: Vec<String> = (0..*captures)
                    .map(|i| format!("f.captures[{i}].clone()"))
                    .collect();
                call_args.extend((0..*arity).map(|_| "args.remove(0)".into()));
                call_args.push("&mut *env".into());
                out.push_str(&format!(
                    "            {CLOSURE_PREFIX}{id}({})\n",
                    call_args.join(", ")
                ));
            }
            ArmSpec::Builtin { name } => out.push_str(&self.builtin_arm(name)),
            ArmSpec::Module {
                name,
                runtime_fn,
                arity,
            } => {
                out.push_str(&Self::arity_guard(name, *arity));
                let call_args: Vec<String> =
                    (0..*arity).map(|_| "&args.remove(0)".into()).collect();
                out.push_str(&format!(
                    "            {runtime_fn}({})\n",
                    call_args.join(", ")
                ));
            }
        }
        out.push_str("        }\n");
        out
    }

    /// The runtime arity check that opens a dispatcher arm.
    pub(super) fn arity_guard(name: &str, arity: usize) -> String {
        format!(
            "            if args.len() != {arity} {{ return Err(function_arity_error(\"{}\", {arity}, args.len())); }}\n",
            escape_str(name)
        )
    }

    /// The body of a builtin dispatcher arm, honoring each builtin's own signature
    /// (some are infallible, `range` takes one or two arguments).
    pub(super) fn builtin_arm(&self, name: &str) -> String {
        let builtin = crate::builtins::builtin(name)
            .expect("compiler bug: dispatcher arm for a name that is not a builtin");
        match builtin.shape {
            BuiltinShape::Fallible => format!(
                "{}            {}(&args.remove(0))\n",
                Self::arity_guard(name, 1),
                builtin.runtime_fn
            ),
            BuiltinShape::Infallible => format!(
                "{}            Ok({}(&args.remove(0)))\n",
                Self::arity_guard(name, 1),
                builtin.runtime_fn
            ),
            BuiltinShape::Range => {
                // `range` accepts one argument (0..n) or two (a..b).
                "            if args.len() != 1 && args.len() != 2 { return Err(function_arity_error(\"range\", 2, args.len())); }\n\
                 \x20           if args.len() == 1 { range(&Value::Int(0i64), &args.remove(0)) } else { range(&args.remove(0), &args.remove(0)) }\n".to_string()
            }
        }
    }
}
