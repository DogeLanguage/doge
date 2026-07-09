//! Doge AST → Rust source. The generated Rust is thin glue: every
//! value operation calls a function in `doge-runtime`, so behaviour lives there,
//! not in these strings.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use crate::ast::{BinOp, Expr, Script, Stmt, UnOp};
use crate::check::BUILTINS;
use crate::diagnostics::Diagnostic;
use crate::stdlib::{self, Module};
use crate::token::Span;

const UNSUPPORTED_HEADLINE: &str = "very soon. much roadmap.";
const ARITY_HEADLINE: &str = "very args. much wrong.";
const RUNTIME_ERROR_HEADLINE: &str = "very error. much broken.";

/// Prefix on every generated variable identifier — makes Rust-keyword
/// collisions impossible. Never appears in anything the user sees.
const NAME_PREFIX: &str = "v_";
/// Prefix on a function's outer wrapper (`f_greet`): guards recursion depth.
const FUNC_PREFIX: &str = "f_";
/// Prefix on a function's body (`b_greet`): the compiled statements. A distinct
/// prefix so a user function named `greet` and one named `b_greet` never clash.
const FUNC_BODY_PREFIX: &str = "b_";
/// Prefix on a closure's outer wrapper (`c_3`): a nested function, keyed by its
/// numeric id so the name can never collide with a user function's `f_` pair.
const CLOSURE_PREFIX: &str = "c_";
/// Prefix on a closure's body (`cb_3`).
const CLOSURE_BODY_PREFIX: &str = "cb_";
/// Prefix on a constructor (`n_0`): builds an instance and runs its `init`.
const CTOR_PREFIX: &str = "n_";
/// Prefix on a method's outer wrapper (`mf_0_speak`). The class-id digit means a
/// method name can never collide with a user function's `f_`/`b_` pair.
const METHOD_PREFIX: &str = "mf_";
/// Prefix on a method's body (`mb_0_speak`).
const METHOD_BODY_PREFIX: &str = "mb_";

/// Turn a checked [`Script`] into a complete Rust source file, or a diagnostic
/// pointing at the first feature M4 cannot run yet. `path`/`source` are used to
/// render those diagnostics and to embed the script path in the uncaught-error
/// message.
pub fn generate(path: &str, source: &str, script: &Script) -> Result<String, Diagnostic> {
    let codegen = Codegen {
        path: path.to_string(),
        lines: source
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect(),
    };
    codegen.file(script)
}

struct Codegen {
    path: String,
    lines: Vec<String>,
}

/// A top-level `many Name:` object definition. `id` is its source-order index —
/// the class tag stored on every instance and matched by the dispatcher.
struct Class {
    name: String,
    id: u32,
    /// Each method as `(name, params)`; `params` excludes the implicit `self`.
    methods: Vec<(String, Vec<String>)>,
}

impl Class {
    /// The parameters of this class's `init`, or an empty slice when it has none
    /// (a class without `init` constructs from zero arguments).
    fn init_params(&self) -> &[String] {
        self.methods
            .iter()
            .find(|(name, _)| name == "init")
            .map(|(_, params)| params.as_slice())
            .unwrap_or(&[])
    }
}

/// The mutable state threaded through one function's (or `run`'s) emission.
struct Emit<'a> {
    /// Every top-level function name → its parameter names (for call resolution
    /// and arity checks).
    funcs: &'a HashMap<String, Vec<String>>,
    /// Every top-level object definition, in source order.
    classes: &'a [Class],
    /// Every imported module name → its table entry.
    modules: &'a HashMap<String, &'static Module>,
    /// The whole-script function analysis: capture info and value ids.
    analysis: &'a Analysis,
    /// Set once any method-call site is compiled, so the dispatcher is emitted
    /// even when a script calls methods but defines no objects of its own.
    uses_method_call: Cell<bool>,
    /// Set once any indirect call is compiled, so the `call_function` dispatcher
    /// is emitted.
    uses_call_function: Cell<bool>,
    /// Every `fn_id` turned into a `Value::function` somewhere — the dispatcher
    /// arms it must carry.
    materialized: RefCell<HashSet<u32>>,
    /// Names local to the code being emitted → how they are stored. Empty while
    /// emitting `run`, where every bound name is an `Env` field.
    locals: HashMap<String, Local>,
    /// Nested-function names in scope → their parameter names, for compile-time
    /// arity checks on direct calls. Reset per callable.
    local_funcs: HashMap<String, Vec<String>>,
    /// Monotonic per-file counter naming `'pN` try labels, `attemptN` binders,
    /// and `'lN` loop labels.
    counter: u32,
    /// Enclosing `pls` labels, innermost last: a fallible call diverts to the
    /// last one instead of `?`-ing out of the function.
    try_stack: Vec<u32>,
    /// Enclosing loop labels, innermost last: `bork`/`continue` target the last.
    loop_stack: Vec<u32>,
}

impl Emit<'_> {
    /// The class named `name`, if one is defined.
    fn class(&self, name: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.name == name)
    }

    /// The imported module named `name`, if one is in scope — but a local of the
    /// same name shadows it (locals always win at a use site).
    fn module(&self, name: &str) -> Option<&'static Module> {
        if self.locals.contains_key(name) {
            None
        } else {
            self.modules.get(name).copied()
        }
    }

    /// The dispatcher id for a bare function name used as a value: a top-level
    /// function, or a builtin. Nested functions are ordinary cell locals, so they
    /// are not handled here.
    fn func_value_id(&self, name: &str) -> Option<u32> {
        self.analysis
            .top_func_ids
            .get(name)
            .copied()
            .or_else(|| self.analysis.builtin_ids.get(name).copied())
    }
}

/// A language feature the parser accepts but the current milestone cannot run
/// yet, with the exact message and the milestone that lands it.
enum Unsupported {
    ClassAsValue(String),
}

impl Unsupported {
    fn detail(&self) -> (String, &'static str) {
        match self {
            Unsupported::ClassAsValue(name) => (
                format!("{name} is an object definition — objects as values land in M6"),
                "M6",
            ),
        }
    }
}

/// How a name is stored in the code currently being emitted: a plain `Value`
/// binding, or a shared `Cell` (because a nested closure captures it).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Local {
    Plain,
    Cell,
}

/// The kind of function a [`FnInfo`] describes, which decides how it is emitted
/// and dispatched.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FnKind {
    /// A direct top-level `such name:` — a static `f_`/`b_` pair, never captures.
    TopLevel,
    /// A nested `such name:` — a `c_`/`cb_` pair that may capture enclosing cells.
    Closure,
    /// An object method — an `mf_`/`mb_` pair, no first-class value, no captures.
    Method,
}

/// The capture/cell analysis for one function definition, keyed by its span.
struct FnInfo {
    /// The dispatcher id, for functions that can become first-class values
    /// (top-level functions and closures). `None` for methods.
    fn_id: Option<u32>,
    name: String,
    /// Declared parameters (for a method, `self` is prepended).
    params: Vec<String>,
    body: Vec<Stmt>,
    /// Names captured from the enclosing scope, in a fixed (sorted) order — the
    /// leading cell parameters this function receives.
    captures: Vec<String>,
    /// Every name that is a `Cell` in this function's own frame: its captures,
    /// its nested-function names, and its locals/params a nested closure captures.
    cell_names: HashSet<String>,
    kind: FnKind,
}

/// A cloned snapshot of a [`FnInfo`]'s emission-relevant fields, so a body can be
/// emitted while `emit` is mutated without holding a borrow into the analysis.
struct FnInfoView {
    name: String,
    params: Vec<String>,
    body: Vec<Stmt>,
    captures: Vec<String>,
    cell_names: HashSet<String>,
}

/// One arm of the generated `call_function` dispatcher, indexed by `fn_id`.
enum ArmSpec {
    /// A top-level function: call its recursion-guarded `f_` wrapper.
    TopFunc { name: String, arity: usize },
    /// A closure: call its `c_` wrapper, threading the captured cells first.
    Closure {
        name: String,
        id: u32,
        arity: usize,
        captures: usize,
    },
    /// A builtin used as a value: call the runtime builtin directly.
    Builtin { name: &'static str },
    /// A stdlib module function used as a value: call its runtime function.
    Module {
        name: String,
        runtime_fn: &'static str,
        arity: usize,
    },
}

/// The whole-script function analysis: per-definition capture info, the
/// dispatcher registry, and the name→id lookups for value construction.
struct Analysis {
    fn_info: HashMap<Span, FnInfo>,
    registry: Vec<ArmSpec>,
    top_func_ids: HashMap<String, u32>,
    builtin_ids: HashMap<&'static str, u32>,
    module_fn_ids: HashMap<(String, String), u32>,
}

impl Codegen {
    fn file(&self, script: &Script) -> Result<String, Diagnostic> {
        // Pre-pass: every top-level function's signature, so a call can resolve
        // (and check its arity) before or after the definition line.
        let mut funcs: HashMap<String, Vec<String>> = HashMap::new();
        for stmt in &script.stmts {
            if let Stmt::FuncDef { name, params, .. } = stmt {
                funcs.insert(name.clone(), params.clone());
            }
        }

        // Pre-pass: every imported module, resolved against the table. An unknown
        // module is a real compile error (with a nudge from `math` to `nerd`).
        // `import_order` keeps source order so module function ids stay stable.
        let mut modules: HashMap<String, &'static Module> = HashMap::new();
        let mut import_order: Vec<(String, &'static Module)> = Vec::new();
        for stmt in &script.stmts {
            if let Stmt::Import { module, span } = stmt {
                match stdlib::module(module) {
                    Some(m) => {
                        modules.insert(module.clone(), m);
                        import_order.push((module.clone(), m));
                    }
                    None => return Err(self.unknown_module(module, *span)),
                }
            }
        }

        let analysis = analyze_script(script, &import_order);

        // Pre-pass: every object definition, in source order — the index is the
        // class id stamped on each instance.
        let mut classes: Vec<Class> = Vec::new();
        for stmt in &script.stmts {
            if let Stmt::ObjDef { name, methods, .. } = stmt {
                let methods = methods
                    .iter()
                    .filter_map(|m| match m {
                        Stmt::FuncDef { name, params, .. } => Some((name.clone(), params.clone())),
                        _ => None,
                    })
                    .collect();
                classes.push(Class {
                    name: name.clone(),
                    id: classes.len() as u32,
                    methods,
                });
            }
        }

        // The `Env` holds the line tracker, the recursion depth, and every
        // top-level bound name — so a function can read and reassign them. A
        // direct top-level definition is a static function, not a field.
        let env_fields = toplevel_hoisted(&script.stmts);

        let mut emit = Emit {
            funcs: &funcs,
            classes: &classes,
            modules: &modules,
            analysis: &analysis,
            uses_method_call: Cell::new(false),
            uses_call_function: Cell::new(false),
            materialized: RefCell::new(HashSet::new()),
            locals: HashMap::new(),
            local_funcs: HashMap::new(),
            counter: 0,
            try_stack: Vec::new(),
            loop_stack: Vec::new(),
        };

        // `run` holds the top-level statements; a top-level function, object, or
        // import emits nothing here — objects become method/constructor helpers
        // below, and imports only wire member calls.
        let mut run_body = String::new();
        for stmt in &script.stmts {
            if matches!(
                stmt,
                Stmt::FuncDef { .. } | Stmt::ObjDef { .. } | Stmt::Import { .. }
            ) {
                continue;
            }
            self.stmt(stmt, 1, &mut emit, &mut run_body)?;
        }

        let mut funcs_src = String::new();
        for stmt in &script.stmts {
            if let Stmt::FuncDef { span, .. } = stmt {
                self.function(*span, &mut emit, &mut funcs_src)?;
            }
        }

        // Each object contributes a constructor plus a wrapper/body pair per
        // method; the source is looked up from the ObjDef, keyed by class id.
        for (class, stmt) in classes.iter().zip(
            script
                .stmts
                .iter()
                .filter(|s| matches!(s, Stmt::ObjDef { .. })),
        ) {
            let Stmt::ObjDef { methods, .. } = stmt else {
                unreachable!("compiler bug: class list and ObjDef filter disagree")
            };
            self.constructor(class, &mut funcs_src);
            for method in methods {
                if let Stmt::FuncDef { span, .. } = method {
                    self.method(class, *span, &mut emit, &mut funcs_src)?;
                }
            }
        }

        // Every closure — nested functions, at any depth — is emitted as a
        // `c_`/`cb_` pair. Ordered by id for stable output.
        let mut closures: Vec<&FnInfo> = analysis
            .fn_info
            .values()
            .filter(|info| info.kind == FnKind::Closure)
            .collect();
        closures.sort_by_key(|info| info.fn_id);
        for info in closures {
            self.closure(info, &mut emit, &mut funcs_src)?;
        }

        let dispatcher = self.dispatcher(&classes, emit.uses_method_call.get());
        let fn_dispatcher = self.function_dispatcher(&emit);

        let mut out = String::new();
        out.push_str("#![allow(warnings)]\n");
        out.push_str("use doge_runtime::*;\n\n");

        // The script's source lines, embedded so an uncaught error can show the
        // offending line without any Rust backtrace. Blanks are kept, so the
        // 1-based line number indexes straight in.
        out.push_str("static LINES: &[&str] = &[\n");
        for line in &self.lines {
            out.push_str(&format!("    \"{}\",\n", escape_str(line)));
        }
        out.push_str("];\n\n");

        out.push_str("struct Env {\n");
        out.push_str("    cur_line: u32,\n");
        out.push_str("    depth: usize,\n");
        for name in &env_fields {
            out.push_str(&format!("    {NAME_PREFIX}{name}: Value,\n"));
        }
        out.push_str("}\n\n");

        out.push_str("fn main() -> std::process::ExitCode {\n");
        out.push_str("    let mut env = Env {\n");
        out.push_str("        cur_line: 0,\n");
        out.push_str("        depth: 0,\n");
        for name in &env_fields {
            out.push_str(&format!("        {NAME_PREFIX}{name}: Value::None,\n"));
        }
        out.push_str("    };\n");
        out.push_str("    match run(&mut env) {\n");
        out.push_str("        Ok(()) => std::process::ExitCode::SUCCESS,\n");
        out.push_str("        Err(e) => {\n");
        out.push_str(
            "            let line = LINES.get((env.cur_line as usize).saturating_sub(1)).copied().unwrap_or(\"\");\n",
        );
        out.push_str(&format!(
            "            eprintln!(\"{}\\n\\n  {}:{{}}\\n    {{line}}\\n  {{e}}\", env.cur_line);\n",
            RUNTIME_ERROR_HEADLINE,
            escape_str(&self.path)
        ));
        out.push_str("            std::process::ExitCode::FAILURE\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");

        out.push_str("fn run(env: &mut Env) -> DogeResult<()> {\n");
        out.push_str(&run_body);
        out.push_str("    Ok(())\n");
        out.push_str("}\n");

        out.push_str(&funcs_src);
        out.push_str(&dispatcher);
        out.push_str(&fn_dispatcher);
        Ok(out)
    }

    /// The "doge has no module named X" diagnostic, nudging `math` toward `nerd`.
    fn unknown_module(&self, name: &str, span: Span) -> Diagnostic {
        let hint = if name == "math" {
            "much math? such nerd — write so nerd".to_string()
        } else {
            format!("modules: {}", stdlib::module_names())
        };
        self.diag(span, format!("doge has no module named {name}"))
            .with_headline("very import. much unknown.")
            .with_hint(hint)
    }

    /// Emit a top-level function as an `f_`/`b_` wrapper + body pair.
    fn function(&self, span: Span, emit: &mut Emit, out: &mut String) -> Result<(), Diagnostic> {
        let info = self.fn_info(emit, span);
        self.emit_callable(
            &format!("{FUNC_PREFIX}{}", info.name),
            &format!("{FUNC_BODY_PREFIX}{}", info.name),
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
    fn method(
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
    fn closure(&self, info: &FnInfo, emit: &mut Emit, out: &mut String) -> Result<(), Diagnostic> {
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

    /// Look up a definition's capture analysis by its span. Cloned because the
    /// borrow would otherwise conflict with mutating `emit` while emitting.
    fn fn_info(&self, emit: &Emit, span: Span) -> FnInfoView {
        let info = emit
            .analysis
            .fn_info
            .get(&span)
            .expect("compiler bug: definition was not analyzed");
        FnInfoView {
            name: info.name.clone(),
            params: info.params.clone(),
            body: info.body.clone(),
            captures: info.captures.clone(),
            cell_names: info.cell_names.clone(),
        }
    }

    /// Emit a wrapper + body pair. The wrapper counts the call against the
    /// recursion limit and undoes it on every exit path — even a `?` inside the
    /// body — because `exit_call` runs after the body returns. Captured cells lead
    /// the parameter list; any local a nested closure captures becomes a `Cell`.
    #[allow(clippy::too_many_arguments)]
    fn emit_callable(
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
    fn constructor(&self, class: &Class, out: &mut String) {
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

    /// Emit the single `call_method` dispatcher: one arm per (class, method),
    /// each checking arity at runtime before calling the method wrapper. Emitted
    /// only when the script defines an object or calls a method somewhere.
    fn dispatcher(&self, classes: &[Class], uses_method_call: bool) -> String {
        if classes.is_empty() && !uses_method_call {
            return String::new();
        }
        let mut out = String::new();
        out.push_str(
            "\nfn call_method(recv: Value, name: &str, mut args: Vec<Value>, env: &mut Env) -> DogeResult<Value> {\n",
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
    fn function_dispatcher(&self, emit: &Emit) -> String {
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
    fn function_arm(&self, id: u32, arm: &ArmSpec) -> String {
        let mut out = String::new();
        out.push_str(&format!("        {id}u32 => {{\n"));
        match arm {
            ArmSpec::TopFunc { name, arity } => {
                out.push_str(&Self::arity_guard(name, *arity));
                let mut call_args: Vec<String> =
                    (0..*arity).map(|_| "args.remove(0)".into()).collect();
                call_args.push("&mut *env".into());
                out.push_str(&format!(
                    "            {FUNC_PREFIX}{name}({})\n",
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
    fn arity_guard(name: &str, arity: usize) -> String {
        format!(
            "            if args.len() != {arity} {{ return Err(function_arity_error(\"{}\", {arity}, args.len())); }}\n",
            escape_str(name)
        )
    }

    /// The body of a builtin dispatcher arm, honoring each builtin's own signature
    /// (some are infallible, `range` takes one or two arguments).
    fn builtin_arm(&self, name: &str) -> String {
        match name {
            "len" => format!(
                "{}            len(&args.remove(0))\n",
                Self::arity_guard("len", 1)
            ),
            "str" => format!(
                "{}            Ok(to_str(&args.remove(0)))\n",
                Self::arity_guard("str", 1)
            ),
            "int" => format!(
                "{}            to_int(&args.remove(0))\n",
                Self::arity_guard("int", 1)
            ),
            "float" => format!(
                "{}            to_float(&args.remove(0))\n",
                Self::arity_guard("float", 1)
            ),
            "range" => {
                // `range` accepts one argument (0..n) or two (a..b).
                "            if args.len() != 1 && args.len() != 2 { return Err(function_arity_error(\"range\", 2, args.len())); }\n\
                 \x20           if args.len() == 1 { range(&Value::Int(0i64), &args.remove(0)) } else { range(&args.remove(0), &args.remove(0)) }\n".to_string()
            }
            _ => unreachable!("compiler bug: unknown builtin in dispatcher"),
        }
    }

    fn stmt(
        &self,
        stmt: &Stmt,
        level: usize,
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let pad = "    ".repeat(level);
        out.push_str(&format!("{pad}env.cur_line = {};\n", stmt_line(stmt)));
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
                        if emit.module(base).is_some() {
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
                    .get(span)
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
    fn emit_bind(&self, emit: &Emit, name: &str, value: &str) -> String {
        match emit.locals.get(name) {
            Some(Local::Cell) => format!("cell_set(&{NAME_PREFIX}{name}, {value});"),
            Some(Local::Plain) => format!("{NAME_PREFIX}{name} = {value};"),
            None => format!("env.{NAME_PREFIX}{name} = {value};"),
        }
    }

    /// Verify an assignment target name is writable: a function, class, or module
    /// name is a fixed binding, not a variable.
    fn check_writable(&self, emit: &Emit, name: &str, span: Span) -> Result<(), Diagnostic> {
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
        if emit.funcs.contains_key(name) {
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
        if emit.module(name).is_some() {
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
    fn resolve_read(&self, emit: &Emit, name: &str) -> String {
        match emit.locals.get(name) {
            Some(Local::Cell) => format!("cell_get(&{NAME_PREFIX}{name})"),
            Some(Local::Plain) => format!("{NAME_PREFIX}{name}.clone()"),
            None => format!("env.{NAME_PREFIX}{name}.clone()"),
        }
    }

    /// Wrap a fallible runtime call so a failure propagates correctly: a plain
    /// `?` at the function level, or a break to the innermost `pls` label when
    /// inside a try body (so `oh no` can catch it instead of unwinding the call).
    fn fail(&self, emit: &Emit, call: String) -> String {
        match emit.try_stack.last() {
            Some(label) => {
                format!("(match {call} {{ Ok(v) => v, Err(e) => break 'p{label} Err(e) }})")
            }
            None => format!("{call}?"),
        }
    }

    /// Codegen an expression to a Rust expression string. Every fallible runtime
    /// call is routed through [`Codegen::fail`].
    fn expr(&self, expr: &Expr, emit: &Emit) -> Result<String, Diagnostic> {
        match expr {
            Expr::Int { value, .. } => Ok(format!("Value::Int({value}i64)")),
            Expr::Float { value, .. } => Ok(format!("Value::Float({value:?}f64)")),
            Expr::Str { value, .. } => Ok(format!("Value::str(\"{}\")", escape_str(value))),
            Expr::Bool { value, .. } => Ok(format!("Value::Bool({value})")),
            Expr::None { .. } => Ok("Value::None".to_string()),
            Expr::Ident { name, span } => {
                if emit.locals.contains_key(name) {
                    // A nested-function name is a `Cell` holding its function
                    // value, so reading it yields that value — no special case.
                    Ok(self.resolve_read(emit, name))
                } else if let Some(id) = emit.func_value_id(name) {
                    emit.materialized.borrow_mut().insert(id);
                    Ok(format!(
                        "Value::function({id}u32, \"{}\", vec![])",
                        escape_str(name)
                    ))
                } else if emit.class(name).is_some() {
                    Err(self.unsupported(*span, Unsupported::ClassAsValue(name.clone())))
                } else if let Some(module) = emit.module(name) {
                    Err(self
                        .diag(*span, format!("{name} is a module, not a value"))
                        .with_headline("very module. much confuse.")
                        .with_hint(format!(
                            "use a member — {name}.{}(…)",
                            module.first_member()
                        )))
                } else {
                    Ok(format!("env.{NAME_PREFIX}{name}.clone()"))
                }
            }
            Expr::List { items, .. } => {
                let mut parts = Vec::with_capacity(items.len());
                for item in items {
                    parts.push(self.expr(item, emit)?);
                }
                Ok(format!("Value::list(vec![{}])", parts.join(", ")))
            }
            Expr::Dict { entries, .. } => {
                let mut pairs = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    pairs.push(format!(
                        "({}, {})",
                        self.expr(key, emit)?,
                        self.expr(value, emit)?
                    ));
                }
                Ok(self.fail(
                    emit,
                    format!("Value::dict_from_pairs(vec![{}])", pairs.join(", ")),
                ))
            }
            Expr::Binary { op, lhs, rhs, .. } => self.binary(*op, lhs, rhs, emit),
            Expr::Unary { op, operand, .. } => {
                let inner = self.expr(operand, emit)?;
                Ok(match op {
                    UnOp::Neg => self.fail(emit, format!("neg({inner})")),
                    UnOp::Not => self.fail(emit, format!("not_({inner})")),
                })
            }
            Expr::Index { obj, index, .. } => {
                let call = format!(
                    "index_get(&{}, &{})",
                    self.expr(obj, emit)?,
                    self.expr(index, emit)?
                );
                Ok(self.fail(emit, call))
            }
            Expr::Call { callee, args, span } => self.call(callee, args, *span, emit),
            Expr::Attr { obj, name, span } => {
                // `module.member` as a value: a const inlines, a function becomes
                // a first-class function value, an unknown member is a real error.
                if let Expr::Ident { name: base, .. } = obj.as_ref() {
                    if let Some(module) = emit.module(base) {
                        if let Some(const_expr) = module.const_expr(name) {
                            return Ok(const_expr.to_string());
                        }
                        if module.func(name).is_some() {
                            let id = emit.analysis.module_fn_ids[&(base.clone(), name.clone())];
                            emit.materialized.borrow_mut().insert(id);
                            return Ok(format!(
                                "Value::function({id}u32, \"{base}.{name}\", vec![])"
                            ));
                        }
                        return Err(self.unknown_member(base, name, module, *span));
                    }
                }
                let call = format!(
                    "attr_get(&{}, \"{}\")",
                    self.expr(obj, emit)?,
                    escape_str(name)
                );
                Ok(self.fail(emit, call))
            }
        }
    }

    /// The "module has no member" diagnostic, listing the members it does have.
    fn unknown_member(
        &self,
        module_name: &str,
        member: &str,
        module: &Module,
        span: Span,
    ) -> Diagnostic {
        self.diag(span, format!("{module_name} has no member {member}"))
            .with_headline("very module. much unknown.")
            .with_hint(format!("{module_name} has: {}", module.members()))
    }

    fn binary(&self, op: BinOp, lhs: &Expr, rhs: &Expr, emit: &Emit) -> Result<String, Diagnostic> {
        // `and`/`or` are Rust block expressions with the right operand INSIDE the
        // guard, so it is evaluated only when the left operand doesn't decide the
        // result. Both always yield a Bool (Doge rule, docs/SYNTAX.md §3).
        if matches!(op, BinOp::And | BinOp::Or) {
            let l = self.expr(lhs, emit)?;
            let r = self.expr(rhs, emit)?;
            return Ok(match op {
                BinOp::And => format!(
                    "{{ let l = {l}; if !l.truthy() {{ Value::Bool(false) }} else {{ Value::Bool(({r}).truthy()) }} }}"
                ),
                BinOp::Or => format!(
                    "{{ let l = {l}; if l.truthy() {{ Value::Bool(true) }} else {{ Value::Bool(({r}).truthy()) }} }}"
                ),
                _ => unreachable!(),
            });
        }
        let l = self.expr(lhs, emit)?;
        let r = self.expr(rhs, emit)?;
        let func = match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
            BinOp::FloorDiv => "floordiv",
            BinOp::Rem => "rem",
            BinOp::Eq => "eq",
            BinOp::NotEq => "ne",
            BinOp::Lt => "lt",
            BinOp::LtEq => "le",
            BinOp::Gt => "gt",
            BinOp::GtEq => "ge",
            BinOp::And | BinOp::Or => unreachable!("handled above"),
        };
        Ok(self.fail(emit, format!("{func}({l}, {r})")))
    }

    fn call(
        &self,
        callee: &Expr,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        match callee {
            Expr::Ident { name, .. } => {
                // A local shadows every other meaning — call through its value. A
                // known nested function still gets a compile-time arity check.
                if emit.locals.contains_key(name) {
                    if let Some(params) = emit.local_funcs.get(name) {
                        self.check_user_arity(name, params, args, span)?;
                    }
                    let callee_val = self.resolve_read(emit, name);
                    return self.indirect_call(&callee_val, args, emit);
                }
                if BUILTINS.contains(&name.as_str()) {
                    return self.builtin_call(name, args, span, emit);
                }
                if emit.funcs.contains_key(name) {
                    let params = &emit.funcs[name];
                    self.check_user_arity(name, params, args, span)?;
                    let mut parts = Vec::with_capacity(args.len() + 1);
                    for arg in args {
                        parts.push(self.expr(arg, emit)?);
                    }
                    parts.push("&mut *env".to_string());
                    let call = format!("{FUNC_PREFIX}{name}({})", parts.join(", "));
                    return Ok(self.fail(emit, call));
                }
                // `Shibe(...)` — a constructor, resolved statically.
                if emit.class(name).is_some() {
                    return self.constructor_call(name, args, span, emit);
                }
                // A module name is not itself callable — you call one of its members.
                if let Some(module) = emit.module(name) {
                    return Err(self
                        .diag(span, format!("{name} is a module, not a function"))
                        .with_headline("very module. much confuse.")
                        .with_hint(format!(
                            "call a member — {name}.{}(…)",
                            module.first_member()
                        )));
                }
                // A top-level variable holding a function value: call it indirectly.
                let callee_val = format!("env.{NAME_PREFIX}{name}.clone()");
                self.indirect_call(&callee_val, args, emit)
            }
            // `nerd.sqrt(16)` — a stdlib member call, when the base is a module.
            Expr::Attr { obj, name, .. } if matches!(obj.as_ref(), Expr::Ident { name: base, .. } if emit.module(base).is_some()) =>
            {
                let Expr::Ident { name: base, .. } = obj.as_ref() else {
                    unreachable!("compiler bug: guarded to an Ident base")
                };
                let module = emit.module(base).expect("compiler bug: module vanished");
                self.module_call(base, module, name, args, span, emit)
            }
            // `kabosu.speak(...)` — a method call, dispatched at runtime.
            Expr::Attr { obj, name, .. } => {
                emit.uses_method_call.set(true);
                let recv = self.expr(obj, emit)?;
                let mut arg_parts = Vec::with_capacity(args.len());
                for arg in args {
                    arg_parts.push(self.expr(arg, emit)?);
                }
                let call = format!(
                    "call_method({recv}, \"{}\", vec![{}], &mut *env)",
                    escape_str(name),
                    arg_parts.join(", ")
                );
                Ok(self.fail(emit, call))
            }
            // Any other callee expression — `f()()`, `xs[0]()` — is called through
            // the value it evaluates to.
            _ => {
                let callee_val = self.expr(callee, emit)?;
                self.indirect_call(&callee_val, args, emit)
            }
        }
    }

    /// Call a function *value*: type-check the callee, then dispatch through
    /// `call_function`. Both steps are fallible and routed through [`Codegen::fail`].
    fn indirect_call(
        &self,
        callee_val: &str,
        args: &[Expr],
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        emit.uses_call_function.set(true);
        let mut arg_parts = Vec::with_capacity(args.len());
        for arg in args {
            arg_parts.push(self.expr(arg, emit)?);
        }
        // `&*` derefs the `Rc<FunctionData>` explicitly: relying on deref coercion
        // through the `?` here trips rustc's expected-type propagation.
        let func = self.fail(emit, format!("callee_function(&{callee_val})"));
        let call = format!(
            "call_function(&*{func}, vec![{}], &mut *env)",
            arg_parts.join(", ")
        );
        Ok(self.fail(emit, call))
    }

    /// A constructor call `Shibe(args)`: static arity against `init`, then a
    /// `n_<id>(args…, &mut *env)` through the fail suffix.
    fn constructor_call(
        &self,
        name: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        let class = emit
            .class(name)
            .expect("compiler bug: constructor for an unknown class");
        let init_params = class.init_params();
        if args.len() != init_params.len() {
            let count = init_params.len();
            let noun = if count == 1 { "argument" } else { "arguments" };
            let hint = if init_params.is_empty() {
                format!("{name}()")
            } else {
                format!("{name}({})", init_params.join(", "))
            };
            return Err(self
                .diag(
                    span,
                    format!("{name} takes {count} {noun}, got {}", args.len()),
                )
                .with_headline(ARITY_HEADLINE)
                .with_hint(hint));
        }
        let mut parts = Vec::with_capacity(args.len() + 1);
        for arg in args {
            parts.push(self.expr(arg, emit)?);
        }
        parts.push("&mut *env".to_string());
        let call = format!("{CTOR_PREFIX}{}({})", class.id, parts.join(", "));
        Ok(self.fail(emit, call))
    }

    /// A stdlib member call `module.member(args)`: static arity against the table,
    /// then `{runtime_fn}(&a0, &a1, …)` through the fail suffix. Calling a const,
    /// or an unknown member, is a real error.
    fn module_call(
        &self,
        module_name: &str,
        module: &Module,
        member: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        let func = match module.func(member) {
            Some(func) => func,
            None => {
                if module.const_expr(member).is_some() {
                    return Err(self
                        .diag(
                            span,
                            format!("{module_name}.{member} is a constant, not a function"),
                        )
                        .with_headline("very module. much confuse.")
                        .with_hint(format!("use it as a value — bark {module_name}.{member}")));
                }
                return Err(self.unknown_member(module_name, member, module, span));
            }
        };
        if args.len() != func.arity {
            let noun = if func.arity == 1 {
                "argument"
            } else {
                "arguments"
            };
            return Err(self
                .diag(
                    span,
                    format!(
                        "{module_name}.{member} takes {} {noun}, got {}",
                        func.arity,
                        args.len()
                    ),
                )
                .with_headline(ARITY_HEADLINE)
                .with_hint(func.hint));
        }
        let mut parts = Vec::with_capacity(args.len());
        for arg in args {
            parts.push(format!("&{}", self.expr(arg, emit)?));
        }
        let call = format!("{}({})", func.runtime_fn, parts.join(", "));
        Ok(self.fail(emit, call))
    }

    fn builtin_call(
        &self,
        name: &str,
        args: &[Expr],
        span: Span,
        emit: &Emit,
    ) -> Result<String, Diagnostic> {
        self.check_builtin_arity(name, args, span)?;
        match name {
            "len" => Ok(self.fail(emit, format!("len(&{})", self.expr(&args[0], emit)?))),
            "str" => Ok(format!("to_str(&{})", self.expr(&args[0], emit)?)),
            "int" => Ok(self.fail(emit, format!("to_int(&{})", self.expr(&args[0], emit)?))),
            "float" => Ok(self.fail(emit, format!("to_float(&{})", self.expr(&args[0], emit)?))),
            "range" if args.len() == 1 => Ok(self.fail(
                emit,
                format!("range(&Value::Int(0i64), &{})", self.expr(&args[0], emit)?),
            )),
            "range" => Ok(self.fail(
                emit,
                format!(
                    "range(&{}, &{})",
                    self.expr(&args[0], emit)?,
                    self.expr(&args[1], emit)?
                ),
            )),
            _ => unreachable!("compiler bug: arity check admitted a non-builtin"),
        }
    }

    /// Builtin arity is statically known: `len`/`str`/`int`/`float` take one
    /// argument, `range` takes one or two.
    fn check_builtin_arity(&self, name: &str, args: &[Expr], span: Span) -> Result<(), Diagnostic> {
        let (ok, expects, hint) = match name {
            "range" => (
                args.len() == 1 || args.len() == 2,
                "1 or 2 arguments",
                "range(n) or range(a, b)".to_string(),
            ),
            _ => (args.len() == 1, "1 argument", format!("{name}(thing)")),
        };
        if ok {
            return Ok(());
        }
        Err(self
            .diag(span, format!("{name} takes {expects}, got {}", args.len()))
            .with_headline(ARITY_HEADLINE)
            .with_hint(hint))
    }

    /// A user function takes exactly its declared parameters; the hint echoes the
    /// call shape doge expected.
    fn check_user_arity(
        &self,
        name: &str,
        params: &[String],
        args: &[Expr],
        span: Span,
    ) -> Result<(), Diagnostic> {
        if args.len() == params.len() {
            return Ok(());
        }
        let count = params.len();
        let noun = if count == 1 { "argument" } else { "arguments" };
        let hint = if params.is_empty() {
            format!("{name}()")
        } else {
            format!("{name}({})", params.join(", "))
        };
        Err(self
            .diag(
                span,
                format!("{name} takes {count} {noun}, got {}", args.len()),
            )
            .with_headline(ARITY_HEADLINE)
            .with_hint(hint))
    }

    fn unsupported(&self, span: Span, feature: Unsupported) -> Diagnostic {
        let (message, milestone) = feature.detail();
        self.diag(span, message)
            .with_headline(UNSUPPORTED_HEADLINE)
            .with_hint(format!(
                "doge check already understands this script — running it lands in {milestone}"
            ))
    }

    fn diag(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        let source_line = self
            .lines
            .get((span.line as usize).saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        Diagnostic::new(&self.path, span.line, span.col, source_line, message)
    }
}

/// Build a comma-joined parameter list: captured cells first (always `Cell`),
/// then value parameters, then the shared `env`. `owned` adds `mut` to value
/// parameters so a body can reassign them; the wrapper takes them plain.
fn signature(captures: &[String], params: &[String], owned: bool) -> String {
    let mut parts: Vec<String> = captures
        .iter()
        .map(|c| format!("{NAME_PREFIX}{c}: Cell"))
        .collect();
    parts.extend(params.iter().map(|p| {
        if owned {
            format!("mut {NAME_PREFIX}{p}: Value")
        } else {
            format!("{NAME_PREFIX}{p}: Value")
        }
    }));
    parts.push("env: &mut Env".to_string());
    parts.join(", ")
}

/// Every bound name in one scope — `such`/`so` declarations, `for` loop
/// variables, `oh no` error names, and nested function definitions — in
/// first-seen order, each once. These become the scope's `Env` fields or hoisted
/// locals. A nested function's own body is not descended into: its inner names
/// belong to its own scope.
pub(crate) fn hoisted_names(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    collect_hoisted(stmts, &mut names);
    names
}

/// The top-level hoisted names for `Env` fields: like [`hoisted_names`] but a
/// direct top-level `such name:` / `many Name:` is a static definition, not an
/// `Env` field, so those direct definitions are skipped (a function nested inside
/// a top-level `if`/`for` block is still a closure and does get a field).
pub(crate) fn toplevel_hoisted(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in stmts {
        if matches!(stmt, Stmt::FuncDef { .. } | Stmt::ObjDef { .. }) {
            continue;
        }
        collect_hoisted(std::slice::from_ref(stmt), &mut names);
    }
    names
}

fn collect_hoisted(stmts: &[Stmt], names: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Decl { name, .. } | Stmt::ConstDecl { name, .. } => push_unique(names, name),
            // A nested function binds its name in this scope; its body is its own.
            Stmt::FuncDef { name, .. } => push_unique(names, name),
            Stmt::For { var, body, .. } => {
                push_unique(names, var);
                collect_hoisted(body, names);
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (_, body) in branches {
                    collect_hoisted(body, names);
                }
                if let Some(body) = else_body {
                    collect_hoisted(body, names);
                }
            }
            Stmt::While { body, .. } => collect_hoisted(body, names),
            Stmt::Try {
                body,
                err_name,
                handler,
                ..
            } => {
                push_unique(names, err_name);
                collect_hoisted(body, names);
                collect_hoisted(handler, names);
            }
            _ => {}
        }
    }
}

fn push_unique(names: &mut Vec<String>, name: &str) {
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
    }
}

/// Every identifier referenced anywhere in an expression (attribute names are
/// dynamic, not variables, so they are not collected).
fn expr_idents(expr: &Expr, out: &mut HashSet<String>) {
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
        Expr::Attr { obj, .. } => expr_idents(obj, out),
        _ => {}
    }
}

/// Names referenced in a body, plus the free names of every nested function it
/// contains (which the enclosing scope must supply). Does not descend into a
/// nested function's own body — that is folded in through its free set.
fn collect_used(stmts: &[Stmt], used: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Decl { expr, .. }
            | Stmt::ConstDecl { expr, .. }
            | Stmt::Bark { expr, .. }
            | Stmt::Bonk { expr, .. }
            | Stmt::ExprStmt { expr } => expr_idents(expr, used),
            Stmt::Assign { target, expr, .. } => {
                expr_idents(target, used);
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
            Stmt::Return {
                expr: Some(expr), ..
            } => expr_idents(expr, used),
            Stmt::FuncDef { params, body, .. } => {
                for name in free_names(params, body) {
                    used.insert(name);
                }
            }
            _ => {}
        }
    }
}

/// The names a function body references but does not bind — the names it needs
/// from an enclosing scope (or that resolve to globals/builtins).
fn free_names(params: &[String], body: &[Stmt]) -> HashSet<String> {
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
fn child_funcdefs(stmts: &[Stmt]) -> Vec<(&str, &[String], &[Stmt], Span)> {
    let mut out = Vec::new();
    collect_child_funcdefs(stmts, &mut out);
    out
}

fn collect_child_funcdefs<'a>(
    stmts: &'a [Stmt],
    out: &mut Vec<(&'a str, &'a [String], &'a [Stmt], Span)>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::FuncDef {
                name,
                params,
                body,
                span,
            } => out.push((name, params, body, *span)),
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (_, body) in branches {
                    collect_child_funcdefs(body, out);
                }
                if let Some(body) = else_body {
                    collect_child_funcdefs(body, out);
                }
            }
            Stmt::For { body, .. } | Stmt::While { body, .. } => collect_child_funcdefs(body, out),
            Stmt::Try { body, handler, .. } => {
                collect_child_funcdefs(body, out);
                collect_child_funcdefs(handler, out);
            }
            _ => {}
        }
    }
}

/// The subset of a scope's own bound names that must be shared cells: every
/// nested-function name, plus any local or parameter a nested closure captures.
fn celled_locals(params: &[String], body: &[Stmt]) -> HashSet<String> {
    let mut candidates: HashSet<String> = params.iter().cloned().collect();
    for name in hoisted_names(body) {
        candidates.insert(name);
    }
    let mut child_free: HashSet<String> = HashSet::new();
    for (_, child_params, child_body, _) in child_funcdefs(body) {
        for name in free_names(child_params, child_body) {
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

/// State threaded through the recursive capture analysis.
struct Analyzer {
    fn_info: HashMap<Span, FnInfo>,
    registry: Vec<ArmSpec>,
    top_func_ids: HashMap<String, u32>,
    next_id: u32,
}

impl Analyzer {
    /// Analyze one function definition: record its capture info, assign its
    /// dispatcher id, and recurse into its nested functions. `enclosing_cells`
    /// are the cell names available in the enclosing frame. Returns this
    /// function's own cell names, so the caller can pass them down.
    fn analyze(
        &mut self,
        name: &str,
        params: &[String],
        body: &[Stmt],
        span: Span,
        kind: FnKind,
        enclosing_cells: &HashSet<String>,
    ) {
        let free = free_names(params, body);
        let mut captures: Vec<String> = free
            .iter()
            .filter(|n| enclosing_cells.contains(*n))
            .cloned()
            .collect();
        captures.sort();

        let mut cell_names = celled_locals(params, body);
        cell_names.extend(captures.iter().cloned());

        let fn_id = match kind {
            FnKind::Method => None,
            FnKind::TopLevel => {
                let id = self.next_id;
                self.next_id += 1;
                self.registry.push(ArmSpec::TopFunc {
                    name: name.to_string(),
                    arity: params.len(),
                });
                self.top_func_ids.insert(name.to_string(), id);
                Some(id)
            }
            FnKind::Closure => {
                let id = self.next_id;
                self.next_id += 1;
                self.registry.push(ArmSpec::Closure {
                    name: name.to_string(),
                    id,
                    arity: params.len(),
                    captures: captures.len(),
                });
                Some(id)
            }
        };

        self.fn_info.insert(
            span,
            FnInfo {
                fn_id,
                name: name.to_string(),
                params: params.to_vec(),
                body: body.to_vec(),
                captures,
                cell_names: cell_names.clone(),
                kind,
            },
        );

        for (child_name, child_params, child_body, child_span) in child_funcdefs(body) {
            self.analyze(
                child_name,
                child_params,
                child_body,
                child_span,
                FnKind::Closure,
                &cell_names,
            );
        }
    }
}

/// Walk the whole script and build the function analysis: capture info per
/// definition, the dispatcher registry, and the name→id lookups. `import_order`
/// lists the imported modules in source order so their function ids are stable.
fn analyze_script(script: &Script, import_order: &[(String, &'static Module)]) -> Analysis {
    let mut analyzer = Analyzer {
        fn_info: HashMap::new(),
        registry: Vec::new(),
        top_func_ids: HashMap::new(),
        next_id: 0,
    };
    let empty = HashSet::new();

    // Direct top-level functions: static, never capture.
    for stmt in &script.stmts {
        if let Stmt::FuncDef {
            name,
            params,
            body,
            span,
        } = stmt
        {
            analyzer.analyze(name, params, body, *span, FnKind::TopLevel, &empty);
        }
    }
    // Object methods: `self` is a bound local; a nested closure may capture it.
    for stmt in &script.stmts {
        if let Stmt::ObjDef { methods, .. } = stmt {
            for method in methods {
                if let Stmt::FuncDef {
                    name,
                    params,
                    body,
                    span,
                } = method
                {
                    let mut with_self = Vec::with_capacity(params.len() + 1);
                    with_self.push("self".to_string());
                    with_self.extend(params.iter().cloned());
                    analyzer.analyze(name, &with_self, body, *span, FnKind::Method, &empty);
                }
            }
        }
    }
    // Functions nested inside a top-level block: closures whose enclosing scope is
    // `run`, which holds no cells — so they capture nothing.
    for stmt in &script.stmts {
        if matches!(
            stmt,
            Stmt::If { .. } | Stmt::For { .. } | Stmt::While { .. } | Stmt::Try { .. }
        ) {
            for (name, params, body, span) in child_funcdefs(std::slice::from_ref(stmt)) {
                analyzer.analyze(name, params, body, span, FnKind::Closure, &empty);
            }
        }
    }

    // Builtins as first-class values, then imported module functions. Their ids
    // follow every user function so user ids stay stable across imports.
    let mut builtin_ids = HashMap::new();
    for builtin in BUILTINS {
        let id = analyzer.next_id;
        analyzer.next_id += 1;
        analyzer.registry.push(ArmSpec::Builtin { name: builtin });
        builtin_ids.insert(*builtin, id);
    }
    let mut module_fn_ids = HashMap::new();
    for (module_name, module) in import_order {
        for func in module.funcs {
            let id = analyzer.next_id;
            analyzer.next_id += 1;
            analyzer.registry.push(ArmSpec::Module {
                name: format!("{module_name}.{}", func.name),
                runtime_fn: func.runtime_fn,
                arity: func.arity,
            });
            module_fn_ids.insert((module_name.clone(), func.name.to_string()), id);
        }
    }

    Analysis {
        fn_info: analyzer.fn_info,
        registry: analyzer.registry,
        top_func_ids: analyzer.top_func_ids,
        builtin_ids,
        module_fn_ids,
    }
}

/// The 1-based source line a statement points at, for `env.cur_line` tracking.
fn stmt_line(stmt: &Stmt) -> u32 {
    match stmt {
        Stmt::Decl { span, .. }
        | Stmt::ConstDecl { span, .. }
        | Stmt::Import { span, .. }
        | Stmt::Assign { span, .. }
        | Stmt::Bark { span, .. }
        | Stmt::If { span, .. }
        | Stmt::For { span, .. }
        | Stmt::While { span, .. }
        | Stmt::FuncDef { span, .. }
        | Stmt::ObjDef { span, .. }
        | Stmt::Try { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Bonk { span, .. }
        | Stmt::Bork { span }
        | Stmt::Continue { span } => span.line,
        Stmt::ExprStmt { expr } => expr.span().line,
    }
}

/// Escape a string so it is a valid Rust string-literal body: backslash, quote,
/// newline and tab become their escape sequences. Used for both Str literals and
/// the embedded script path.
fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn gen(source: &str) -> Result<String, Diagnostic> {
        let script = parse("examples/hello.doge", source).expect("parse should succeed");
        generate("examples/hello.doge", source, &script)
    }

    #[test]
    fn golden_hello_output() {
        let out = gen("such age = 7\nbark \"age is \" + str(age)\nwow\n").unwrap();
        let expected = "\
#![allow(warnings)]
use doge_runtime::*;

static LINES: &[&str] = &[
    \"such age = 7\",
    \"bark \\\"age is \\\" + str(age)\",
    \"wow\",
    \"\",
];

struct Env {
    cur_line: u32,
    depth: usize,
    v_age: Value,
}

fn main() -> std::process::ExitCode {
    let mut env = Env {
        cur_line: 0,
        depth: 0,
        v_age: Value::None,
    };
    match run(&mut env) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            let line = LINES.get((env.cur_line as usize).saturating_sub(1)).copied().unwrap_or(\"\");
            eprintln!(\"very error. much broken.\\n\\n  examples/hello.doge:{}\\n    {line}\\n  {e}\", env.cur_line);
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(env: &mut Env) -> DogeResult<()> {
    env.cur_line = 1;
    env.v_age = Value::Int(7i64);
    env.cur_line = 2;
    let _ = bark(&add(Value::str(\"age is \"), to_str(&env.v_age.clone()))?);
    Ok(())
}
";
        assert_eq!(out, expected);
    }

    #[test]
    fn golden_function_shape() {
        let out = gen("such greet much name:\n    return name\nwow\nbark greet(\"kabosu\")\nwow\n")
            .unwrap();
        let expected = "\
#![allow(warnings)]
use doge_runtime::*;

static LINES: &[&str] = &[
    \"such greet much name:\",
    \"    return name\",
    \"wow\",
    \"bark greet(\\\"kabosu\\\")\",
    \"wow\",
    \"\",
];

struct Env {
    cur_line: u32,
    depth: usize,
}

fn main() -> std::process::ExitCode {
    let mut env = Env {
        cur_line: 0,
        depth: 0,
    };
    match run(&mut env) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            let line = LINES.get((env.cur_line as usize).saturating_sub(1)).copied().unwrap_or(\"\");
            eprintln!(\"very error. much broken.\\n\\n  examples/hello.doge:{}\\n    {line}\\n  {e}\", env.cur_line);
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(env: &mut Env) -> DogeResult<()> {
    env.cur_line = 4;
    let _ = bark(&f_greet(Value::str(\"kabosu\"), &mut *env)?);
    Ok(())
}

fn f_greet(v_name: Value, env: &mut Env) -> DogeResult<Value> {
    enter_call(&mut env.depth)?;
    let result = b_greet(v_name, env);
    exit_call(&mut env.depth);
    result
}

fn b_greet(mut v_name: Value, env: &mut Env) -> DogeResult<Value> {
    env.cur_line = 2;
    return Ok(v_name.clone());
    Ok(Value::None)
}
";
        assert_eq!(out, expected);
    }

    #[test]
    fn decl_inside_if_is_hoisted() {
        let out = gen("such c = 1\nif c:\n    such y = 2\nbark y\nwow\n").unwrap();
        assert!(out.contains("    v_y: Value,\n"));
        assert!(out.contains("env.v_y = Value::Int(2i64);"));
        assert!(out.contains("let _ = bark(&env.v_y.clone());"));
    }

    #[test]
    fn for_variable_is_hoisted() {
        let out = gen("such xs = [1, 2]\nfor x in xs:\n    bark x\nwow\n").unwrap();
        assert!(out.contains("    v_x: Value,\n"));
        assert!(out.contains("'l0: for item in iter_value(&env.v_xs.clone())? {"));
        assert!(out.contains("env.v_x = item;"));
    }

    #[test]
    fn and_or_short_circuit_shape() {
        let and = gen("such a = true\nsuch b = false\nbark a and b\nwow\n").unwrap();
        assert!(and.contains(
            "{ let l = env.v_a.clone(); if !l.truthy() { Value::Bool(false) } else { Value::Bool((env.v_b.clone()).truthy()) } }"
        ));
        let or = gen("such a = true\nsuch b = false\nbark a or b\nwow\n").unwrap();
        assert!(or.contains(
            "{ let l = env.v_a.clone(); if l.truthy() { Value::Bool(true) } else { Value::Bool((env.v_b.clone()).truthy()) } }"
        ));
    }

    #[test]
    fn rust_keyword_idents_are_mangled() {
        // `match` is a Rust keyword; the `v_` prefix keeps the generated code legal.
        let out = gen("such match = 1\nbark match\nwow\n").unwrap();
        assert!(out.contains("    v_match: Value,\n"));
        assert!(out.contains("env.v_match = Value::Int(1i64);"));
    }

    #[test]
    fn string_escapes_survive() {
        let out = gen("such s = \"a\\\"b\\nc\"\nwow\n").unwrap();
        // The Doge string a"b<newline>c becomes an escaped Rust string literal.
        assert!(out.contains("Value::str(\"a\\\"b\\nc\")"));
    }

    #[test]
    fn const_compiles_like_decl() {
        let out = gen("so PI = 3\nbark PI\nwow\n").unwrap();
        assert!(out.contains("    v_PI: Value,\n"));
        assert!(out.contains("env.v_PI = Value::Int(3i64);"));
        assert!(out.contains("let _ = bark(&env.v_PI.clone());"));
    }

    #[test]
    fn try_block_shape() {
        let out =
            gen("such x = 0\npls\n    very x = 1 // 0\noh no err!\n    bark err\nwow\n").unwrap();
        assert!(out.contains("let attempt0: DogeResult<()> = 'p0: {"));
        assert!(out.contains("Err(e) => break 'p0 Err(e)"));
        assert!(out.contains("if let Err(e) = attempt0 {"));
        assert!(out.contains("env.v_err = error_value(&e);"));
    }

    #[test]
    fn bonk_returns_err() {
        let out = gen("bonk \"nope\"\nwow\n").unwrap();
        assert!(out.contains("return Err(bonk_error(&Value::str(\"nope\")));"));
    }

    #[test]
    fn bonk_in_try_breaks_to_label() {
        let out = gen("pls\n    bonk \"nope\"\noh no err!\n    bark err\nwow\n").unwrap();
        assert!(out.contains("break 'p0 Err(bonk_error(&Value::str(\"nope\")));"));
    }

    #[test]
    fn loops_are_labeled_and_bork_uses_labels() {
        // A bork inside a pls inside a for must break the labeled loop, crossing
        // the labeled try block.
        let out =
            gen("such xs = [1]\nfor x in xs:\n    pls\n        bork\n    oh no err!\n        bark err\nwow\n")
                .unwrap();
        assert!(out.contains("'l0: for item in"));
        assert!(out.contains("'p1: {"));
        assert!(out.contains("break 'l0;"));
    }

    #[test]
    fn builtin_arity_error_is_precise() {
        let err = gen("bark len(1, 2, 3)\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very args. much wrong.");
        assert_eq!(err.message, "len takes 1 argument, got 3");
        assert_eq!(err.hint.as_deref(), Some("len(thing)"));

        let range_err = gen("bark range(1, 2, 3)\nwow\n").unwrap_err();
        assert_eq!(range_err.message, "range takes 1 or 2 arguments, got 3");
    }

    #[test]
    fn function_arity_error_is_precise() {
        let err =
            gen("such add2 much a, b:\n    return a + b\nwow\nbark add2(1)\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very args. much wrong.");
        assert_eq!(err.message, "add2 takes 2 arguments, got 1");
        assert_eq!(err.hint.as_deref(), Some("add2(a, b)"));
    }

    #[test]
    fn range_one_and_two_args() {
        let one = gen("for i in range(3):\n    bark i\nwow\n").unwrap();
        assert!(one.contains("range(&Value::Int(0i64), &Value::Int(3i64))?"));
        let two = gen("for i in range(2, 5):\n    bark i\nwow\n").unwrap();
        assert!(two.contains("range(&Value::Int(2i64), &Value::Int(5i64))?"));
    }

    #[test]
    fn function_as_value_constructs_a_function_value() {
        let out = gen("such greet:\n    bark 1\nwow\nsuch g = greet\nwow\n").unwrap();
        // A top-level function name used as a value builds a `Value::function`.
        assert!(out.contains("env.v_g = Value::function(0u32, \"greet\", vec![]);"));
    }

    #[test]
    fn builtin_as_value_constructs_a_function_value() {
        // `bark len` — a bare builtin name used as a value.
        let out = gen("bark len\nwow\n").unwrap();
        assert!(out.contains("Value::function(0u32, \"len\", vec![])"));
    }

    #[test]
    fn indirect_call_goes_through_the_dispatcher() {
        let out = gen("such x = 1\nx()\nwow\n").unwrap();
        assert!(
            out.contains("call_function(&*callee_function(&env.v_x.clone())?, vec![], &mut *env)?")
        );
        assert!(out.contains("fn call_function(f: &FunctionData"));
    }

    #[test]
    fn nested_funcdef_becomes_a_closure() {
        let out = gen("such outer:\n    such inner:\n        bark 1\n    wow\nwow\nwow\n").unwrap();
        // The nested name is a hoisted cell, set to a closure value; the closure
        // body is emitted as a `c_`/`cb_` pair.
        assert!(out.contains("let v_inner: Cell = Rc::new(RefCell::new(Value::None));"));
        assert!(out.contains("cell_set(&v_inner, Value::function(1u32, \"inner\", vec![]));"));
        assert!(out.contains("fn c_1(env: &mut Env)"));
        assert!(out.contains("fn cb_1(env: &mut Env)"));
    }

    #[test]
    fn closure_captures_an_enclosing_variable() {
        // `bump` reads and writes `count`, which becomes a shared cell in `outer`.
        let out = gen(
            "such outer:\n    such count = 0\n    such bump:\n        very count = count + 1\n        return count\n    wow\n    return bump()\nwow\nwow\n",
        )
        .unwrap();
        assert!(out.contains("let v_count: Cell = Rc::new(RefCell::new(Value::None));"));
        // The closure receives `count` as a leading cell parameter.
        assert!(out.contains("fn cb_1(v_count: Cell, env: &mut Env)"));
        assert!(out.contains("cell_set(&v_count, add(cell_get(&v_count), Value::Int(1i64))?);"));
        // Construction threads the shared cell into the function value.
        assert!(out.contains(
            "cell_set(&v_bump, Value::function(1u32, \"bump\", vec![v_count.clone()]));"
        ));
    }

    #[test]
    fn direct_nested_call_keeps_compile_time_arity() {
        let err = gen(
            "such outer:\n    such add2 much a, b:\n        return a + b\n    wow\n    return add2(1)\nwow\nwow\n",
        )
        .unwrap_err();
        assert_eq!(err.headline, "very args. much wrong.");
        assert_eq!(err.message, "add2 takes 2 arguments, got 1");
    }

    #[test]
    fn reassigning_a_nested_function_is_an_error() {
        let err = gen(
            "such outer:\n    such inner:\n        bark 1\n    wow\n    very inner = 5\nwow\nwow\n",
        )
        .unwrap_err();
        assert_eq!(err.headline, "very function. much fixed.");
    }

    #[test]
    fn module_func_as_value_constructs_a_function_value() {
        let out = gen("so nerd\nsuch s = nerd.sqrt\nwow\n").unwrap();
        assert!(out.contains("Value::function("));
        assert!(out.contains("\"nerd.sqrt\", vec![]"));
    }

    #[test]
    fn so_math_hints_at_nerd() {
        let err = gen("so math\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very import. much unknown.");
        assert!(err.hint.as_deref().unwrap_or_default().contains("so nerd"));
    }

    #[test]
    fn unknown_module_is_an_error() {
        let err = gen("so bogus\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very import. much unknown.");
        assert_eq!(err.message, "doge has no module named bogus");
        assert!(err
            .hint
            .as_deref()
            .unwrap_or_default()
            .contains("nerd, strings, lists"));
    }

    #[test]
    fn module_call_emits_runtime_fn() {
        let out = gen("so nerd\nbark nerd.sqrt(16)\nwow\n").unwrap();
        assert!(out.contains("nerd_sqrt(&Value::Int(16i64))?"));
    }

    #[test]
    fn module_const_emits_value() {
        let out = gen("so nerd\nbark nerd.pi\nwow\n").unwrap();
        assert!(out.contains("Value::Float(std::f64::consts::PI)"));
    }

    #[test]
    fn unknown_member_is_an_error() {
        let err = gen("so nerd\nbark nerd.bogus(1)\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very module. much unknown.");
        assert_eq!(err.message, "nerd has no member bogus");
    }

    #[test]
    fn module_member_arity_error_is_precise() {
        let err = gen("so nerd\nbark nerd.sqrt(1, 2)\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very args. much wrong.");
        assert_eq!(err.message, "nerd.sqrt takes 1 argument, got 2");
        assert_eq!(err.hint.as_deref(), Some("nerd.sqrt(x)"));
    }

    #[test]
    fn module_const_called_is_an_error() {
        let err = gen("so nerd\nbark nerd.pi(1)\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very module. much confuse.");
        assert_eq!(err.message, "nerd.pi is a constant, not a function");
    }

    #[test]
    fn module_as_value_is_an_error() {
        let err = gen("so nerd\nbark nerd\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very module. much confuse.");
        assert_eq!(err.message, "nerd is a module, not a value");
    }

    #[test]
    fn bare_module_func_is_a_value() {
        // `bark nerd.sqrt` prints the function value rather than erroring.
        let out = gen("so nerd\nbark nerd.sqrt\nwow\n").unwrap();
        assert!(out.contains("Value::function("));
        assert!(out.contains("\"nerd.sqrt\", vec![]"));
    }

    #[test]
    fn calling_a_module_is_an_error() {
        let err = gen("so nerd\nbark nerd(1)\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very module. much confuse.");
        assert_eq!(err.message, "nerd is a module, not a function");
    }

    #[test]
    fn assign_to_module_name_is_an_error() {
        let err = gen("so nerd\nnerd = 5\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very module. much fixed.");
    }

    #[test]
    fn assign_into_module_is_an_error() {
        let err = gen("so nerd\nnerd.x = 5\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very module. much fixed.");
        assert_eq!(err.message, "cannot assign into a module");
    }

    #[test]
    fn nested_import_is_an_error() {
        let err = gen("such f:\n    so nerd\nwow\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very nested. much import.");
    }

    #[test]
    fn assign_to_function_name_is_an_error() {
        let err = gen("such greet:\n    bark 1\nwow\nvery greet = 5\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very function. much fixed.");
    }

    #[test]
    fn fn_local_vs_global_resolution() {
        // The function reassigns a top-level name (env field) and declares its
        // own local (a plain `v_`).
        let out = gen(
            "such total = 0\nsuch tally much n:\n    such step = n\n    very total = total + step\n    return total\nwow\nbark tally(2)\nwow\n",
        )
        .unwrap();
        assert!(out.contains("let mut v_step: Value = Value::None;"));
        assert!(out.contains("env.v_total = add(env.v_total.clone(), v_step.clone())?;"));
    }

    #[test]
    fn bare_return_and_missing_return_yield_none() {
        let out = gen("such f:\n    return\nwow\nf()\nwow\n").unwrap();
        assert!(out.contains("return Ok(Value::None);"));
        // The body still ends with the fall-off-end none.
        assert!(out.contains("    Ok(Value::None)\n}\n"));
    }

    #[test]
    fn object_golden_shape() {
        let src = "many Shibe:\n    such init much name, age:\n        self.name = name\n        self.age = age\n    wow\n\n    such speak:\n        bark self.name + \" says bork\"\n    wow\nwow\n\nsuch kabosu = Shibe(\"kabosu\", 18)\nkabosu.speak()\nwow\n";
        let out = gen(src).unwrap();
        let expected = r#"#![allow(warnings)]
use doge_runtime::*;

static LINES: &[&str] = &[
    "many Shibe:",
    "    such init much name, age:",
    "        self.name = name",
    "        self.age = age",
    "    wow",
    "",
    "    such speak:",
    "        bark self.name + \" says bork\"",
    "    wow",
    "wow",
    "",
    "such kabosu = Shibe(\"kabosu\", 18)",
    "kabosu.speak()",
    "wow",
    "",
];

struct Env {
    cur_line: u32,
    depth: usize,
    v_kabosu: Value,
}

fn main() -> std::process::ExitCode {
    let mut env = Env {
        cur_line: 0,
        depth: 0,
        v_kabosu: Value::None,
    };
    match run(&mut env) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            let line = LINES.get((env.cur_line as usize).saturating_sub(1)).copied().unwrap_or("");
            eprintln!("very error. much broken.\n\n  examples/hello.doge:{}\n    {line}\n  {e}", env.cur_line);
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(env: &mut Env) -> DogeResult<()> {
    env.cur_line = 12;
    env.v_kabosu = n_0(Value::str("kabosu"), Value::Int(18i64), &mut *env)?;
    env.cur_line = 13;
    let _ = call_method(env.v_kabosu.clone(), "speak", vec![], &mut *env)?;
    Ok(())
}

fn n_0(v_name: Value, v_age: Value, env: &mut Env) -> DogeResult<Value> {
    let obj = Value::object(0u32, "Shibe");
    mf_0_init(obj.clone(), v_name, v_age, env)?;
    Ok(obj)
}

fn mf_0_init(v_self: Value, v_name: Value, v_age: Value, env: &mut Env) -> DogeResult<Value> {
    enter_call(&mut env.depth)?;
    let result = mb_0_init(v_self, v_name, v_age, env);
    exit_call(&mut env.depth);
    result
}

fn mb_0_init(mut v_self: Value, mut v_name: Value, mut v_age: Value, env: &mut Env) -> DogeResult<Value> {
    env.cur_line = 3;
    attr_set(&v_self.clone(), "name", v_name.clone())?;
    env.cur_line = 4;
    attr_set(&v_self.clone(), "age", v_age.clone())?;
    Ok(Value::None)
}

fn mf_0_speak(v_self: Value, env: &mut Env) -> DogeResult<Value> {
    enter_call(&mut env.depth)?;
    let result = mb_0_speak(v_self, env);
    exit_call(&mut env.depth);
    result
}

fn mb_0_speak(mut v_self: Value, env: &mut Env) -> DogeResult<Value> {
    env.cur_line = 8;
    let _ = bark(&add(attr_get(&v_self.clone(), "name")?, Value::str(" says bork"))?);
    Ok(Value::None)
}

fn call_method(recv: Value, name: &str, mut args: Vec<Value>, env: &mut Env) -> DogeResult<Value> {
    match (object_class_id(&recv, name)?, name) {
        (0u32, "init") => {
            if args.len() != 2 { return Err(method_arity_error("Shibe", "init", 2, args.len())); }
            mf_0_init(recv, args.remove(0), args.remove(0), env)
        }
        (0u32, "speak") => {
            if args.len() != 0 { return Err(method_arity_error("Shibe", "speak", 0, args.len())); }
            mf_0_speak(recv, env)
        }
        _ => Err(no_such_method(&recv, name)),
    }
}
"#;
        assert_eq!(out, expected);
    }

    #[test]
    fn attr_get_and_set_emission() {
        let out = gen("such x = 1\nx.name = 2\nbark x.name\nwow\n").unwrap();
        assert!(out.contains("attr_set(&env.v_x.clone(), \"name\", Value::Int(2i64))?;"));
        assert!(out.contains("attr_get(&env.v_x.clone(), \"name\")?"));
    }

    #[test]
    fn attr_in_try_breaks_to_label() {
        let out = gen("such x = 1\npls\n    bark x.name\noh no err!\n    bark err\nwow\n").unwrap();
        assert!(out.contains(
            "match attr_get(&env.v_x.clone(), \"name\") { Ok(v) => v, Err(e) => break 'p0 Err(e) }"
        ));
    }

    #[test]
    fn method_call_is_dynamic() {
        let out =
            gen("many S:\n    such go:\n        bark 1\n    wow\nwow\nsuch a = S()\na.go()\nwow\n")
                .unwrap();
        assert!(out.contains("call_method(env.v_a.clone(), \"go\", vec![], &mut *env)?"));
        assert!(out.contains("object_class_id(&recv, name)?"));
    }

    #[test]
    fn self_resolves_to_param() {
        let out = gen("many Shibe:\n    such speak:\n        bark self\n    wow\nwow\nsuch k = Shibe()\nk.speak()\nwow\n").unwrap();
        assert!(out.contains("fn mb_0_speak(mut v_self: Value, env: &mut Env)"));
        assert!(out.contains("bark(&v_self.clone())"));
    }

    #[test]
    fn constructor_arity_error_is_precise() {
        let err = gen("many Shibe:\n    such init much name, age:\n        self.name = name\n    wow\nwow\nsuch k = Shibe(\"x\")\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very args. much wrong.");
        assert_eq!(err.message, "Shibe takes 2 arguments, got 1");
        assert_eq!(err.hint.as_deref(), Some("Shibe(name, age)"));
    }

    #[test]
    fn no_init_class_takes_no_args() {
        let out =
            gen("many Thing:\n    such go:\n        bark 1\n    wow\nwow\nsuch t = Thing()\nwow\n")
                .unwrap();
        assert!(out.contains("fn n_0(env: &mut Env) -> DogeResult<Value> {"));
        assert!(out.contains("let obj = Value::object(0u32, \"Thing\");"));
        assert!(!out.contains("mf_0_init"));
        let err = gen(
            "many Thing:\n    such go:\n        bark 1\n    wow\nwow\nsuch t = Thing(1)\nwow\n",
        )
        .unwrap_err();
        assert_eq!(err.message, "Thing takes 0 arguments, got 1");
        assert_eq!(err.hint.as_deref(), Some("Thing()"));
    }

    #[test]
    fn class_as_value_lands_m6() {
        let err =
            gen("many Shibe:\n    such go:\n        bark 1\n    wow\nwow\nsuch g = Shibe\nwow\n")
                .unwrap_err();
        assert_eq!(err.headline, "very soon. much roadmap.");
        assert!(err.message.contains("Shibe is an object definition"));
        assert!(err.hint.as_deref().unwrap_or_default().contains("M6"));
    }

    #[test]
    fn assign_to_class_name_is_an_error() {
        let err =
            gen("many Shibe:\n    such go:\n        bark 1\n    wow\nwow\nvery Shibe = 5\nwow\n")
                .unwrap_err();
        assert_eq!(err.headline, "very object. much fixed.");
    }

    #[test]
    fn nested_objdef_is_an_error() {
        let err = gen("such f:\n    many Inner:\n        such g:\n            bark 1\n        wow\n    wow\nwow\nwow\n").unwrap_err();
        assert_eq!(err.headline, "very nested. much object.");
    }

    #[test]
    fn lines_static_escapes_quotes() {
        let out = gen("bark \"hi\"\nwow\n").unwrap();
        assert!(out.contains("static LINES: &[&str] = &["));
        assert!(out.contains(r#"    "bark \"hi\"","#));
    }

    #[test]
    fn no_dispatcher_without_objects() {
        let out = gen("bark 1\nwow\n").unwrap();
        assert!(!out.contains("fn call_method"));
    }
}
