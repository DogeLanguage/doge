use super::*;
use crate::parser::parse;

fn gen(source: &str) -> Result<String, Diagnostic> {
    let script = parse("examples/hello.doge", source).expect("parse should succeed");
    let program = crate::modules::single_file_program("examples/hello.doge", source, script)?;
    generate_program(&program)
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
    let out =
        gen("such greet much name:\n    return name\nwow\nbark greet(\"kabosu\")\nwow\n").unwrap();
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
fn destructuring_declaration_unpacks_into_each_binding() {
    let out = gen("such a, b = [1, 2]\nbark a\nwow\n").unwrap();
    assert!(out.contains("let vals0 = unpack_value(&src0, 2, false)?;"));
    assert!(out.contains("env.v_a = vals0[0].clone();"));
    assert!(out.contains("env.v_b = vals0[1].clone();"));
}

#[test]
fn destructuring_with_a_collector_passes_the_rest_flag() {
    let out = gen("such first, many rest = [1, 2, 3]\nbark first\nwow\n").unwrap();
    assert!(out.contains("let vals0 = unpack_value(&src0, 1, true)?;"));
    assert!(out.contains("env.v_first = vals0[0].clone();"));
    assert!(out.contains("env.v_rest = vals0[1].clone();"));
}

#[test]
fn for_loop_destructuring_unpacks_each_item() {
    let out = gen("such d = {\"a\": 1}\nfor k, v in d:\n    bark k\nwow\n").unwrap();
    assert!(out.contains("'l0: for item in iter_value(&env.v_d.clone())? {"));
    assert!(out.contains("let vals1 = unpack_value(&src1, 2, false)?;"));
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
    let out = gen("such x = 0\npls\n    very x = 1 // 0\noh no err!\n    bark err\nwow\n").unwrap();
    assert!(out.contains("let attempt0: DogeResult<()> = 'p0: {"));
    assert!(out.contains("Err(e) => break 'p0 Err(e)"));
    assert!(out.contains("if let Err(e) = attempt0 {"));
    assert!(out.contains("env.v_err = error_value(&e, \"examples/hello.doge\", env.cur_line);"));
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
fn interpolation_emits_an_interp_call() {
    let out = gen("such name = \"kabosu\"\nbark \"hi {name}, {1 + 1}\"\nwow\n").unwrap();
    assert!(out.contains("interp(&["));
    // The literal segments survive escaping and the holes compile as exprs.
    assert!(out.contains("Value::str(\"hi \")"));
    assert!(out.contains("add("));
}

#[test]
fn interpolation_literal_escapes_survive() {
    // A literal `"` inside the interpolated text must stay escaped in the
    // generated Rust string literal.
    let out = gen("bark \"a \\\" {1}\"\nwow\n").unwrap();
    assert!(out.contains("Value::str(\"a \\\" \")"));
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
    let err = gen("such add2 much a, b:\n    return a + b\nwow\nbark add2(1)\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very args. much wrong.");
    assert_eq!(err.message, "add2 takes 2 arguments, got 1");
    assert_eq!(err.hint.as_deref(), Some("add2(a, b)"));
}

#[test]
fn default_is_filled_at_a_direct_call() {
    let out = gen("such greet much name, mood = \"happy\":\n    return name\nwow\nbark greet(\"kabosu\")\nwow\n")
        .unwrap();
    // The omitted `mood` argument is supplied from its literal default.
    assert!(out.contains("f_greet(Value::str(\"kabosu\"), Value::str(\"happy\"), &mut *env)"));
}

#[test]
fn variadic_packs_the_surplus_into_a_list() {
    let out = gen(
        "such party much host, many rest:\n    return host\nwow\nparty(\"a\", \"b\", \"c\")\nwow\n",
    )
    .unwrap();
    assert!(out.contains(
        "f_party(Value::str(\"a\"), Value::list(vec![Value::str(\"b\"), Value::str(\"c\")]), &mut *env)"
    ));
}

#[test]
fn keyword_args_bind_by_name_with_ordered_temporaries() {
    let out = gen("such f much a, b, c:\n    return a\nwow\nf(1, c = 3, b = 2)\nwow\n").unwrap();
    // Provided arguments evaluate left-to-right into temporaries, then fill their
    // slots in binding order (a, b, c) → temp for `c` lands in the third slot.
    assert!(out.contains("let __a0 = Value::Int(1i64); "));
    assert!(out.contains("let __a1 = Value::Int(3i64); "));
    assert!(out.contains("let __a2 = Value::Int(2i64); "));
    assert!(out.contains("f_f(__a0, __a2, __a1, &mut *env)"));
}

#[test]
fn range_arity_error_reports_the_accepted_span() {
    let err =
        gen("such greet much name, mood = \"happy\":\n    return name\nwow\nbark greet()\nwow\n")
            .unwrap_err();
    assert_eq!(err.headline, "very args. much wrong.");
    assert_eq!(err.message, "greet takes 1 to 2 arguments, got 0");
}

#[test]
fn variadic_arity_error_says_at_least() {
    let err =
        gen("such party much host, many rest:\n    return host\nwow\nparty()\nwow\n").unwrap_err();
    assert_eq!(err.message, "party takes at least 1 argument, got 0");
}

#[test]
fn unknown_keyword_argument_is_an_error() {
    let err =
        gen("such greet much name:\n    return name\nwow\ngreet(mood = 1)\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very keyword. much unknown.");
}

#[test]
fn keyword_argument_on_a_method_is_rejected() {
    let src = "many Shibe:\n    such speak much mood:\n        return mood\n    wow\nwow\nsuch k = Shibe()\nk.speak(mood = 1)\nwow\n";
    let err = gen(src).unwrap_err();
    assert_eq!(err.headline, "very keyword. much dynamic.");
}

#[test]
fn dispatcher_arm_fills_defaults_and_packs_variadic() {
    // Called through a value, so the arm — not the call site — fills the default.
    let out = gen("such greet much name, mood = \"happy\":\n    return name\nwow\nsuch g = greet\nbark g(\"kabosu\")\nwow\n")
        .unwrap();
    assert!(out.contains("if args.len() < 1 { return Err(function_arity_error(\"greet\", 1usize, Some(2usize), args.len())); }"));
    assert!(out.contains("if args.len() < 2 { args.push(Value::str(\"happy\")); }"));
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
    assert!(out.contains("call_function(&*callee_function(&env.v_x.clone())?, vec![], &mut *env)?"));
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
    assert!(
        out.contains("cell_set(&v_bump, Value::function(1u32, \"bump\", vec![v_count.clone()]));")
    );
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
        .contains("nerd, strings"));
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
    if !matches!(recv, Value::Object(_)) { return builtin_method(&recv, name, args); }
    match (object_class_id(&recv)?, name) {
        (0u32, "init") => {
            if args.len() != 2 { return Err(method_arity_error("Shibe", "init", 2usize, Some(2usize), args.len())); }
            mf_0_init(recv, args.remove(0), args.remove(0), env)
        }
        (0u32, "speak") => {
            if args.len() != 0 { return Err(method_arity_error("Shibe", "speak", 0usize, Some(0usize), args.len())); }
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
    assert!(out.contains("object_class_id(&recv)?"));
    assert!(out.contains(
        "if !matches!(recv, Value::Object(_)) { return builtin_method(&recv, name, args); }"
    ));
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
    let err =
        gen("many Thing:\n    such go:\n        bark 1\n    wow\nwow\nsuch t = Thing(1)\nwow\n")
            .unwrap_err();
    assert_eq!(err.message, "Thing takes 0 arguments, got 1");
    assert_eq!(err.hint.as_deref(), Some("Thing()"));
}

#[test]
fn class_as_value_is_an_error() {
    let err = gen("many Shibe:\n    such go:\n        bark 1\n    wow\nwow\nsuch g = Shibe\nwow\n")
        .unwrap_err();
    assert_eq!(err.headline, "very class. much value.");
    assert!(err
        .message
        .contains("Shibe is an object definition, not a value"));
    assert!(err.hint.as_deref().unwrap_or_default().contains("Shibe(…)"));
}

#[test]
fn assign_to_class_name_is_an_error() {
    let err = gen("many Shibe:\n    such go:\n        bark 1\n    wow\nwow\nvery Shibe = 5\nwow\n")
        .unwrap_err();
    assert_eq!(err.headline, "very object. much fixed.");
}

#[test]
fn nested_objdef_is_an_error() {
    let err = gen("such f:\n    many Inner:\n        such g:\n            bark 1\n        wow\n    wow\nwow\nwow\n").unwrap_err();
    assert_eq!(err.headline, "very nested. much object.");
}

/// A child dispatches its inherited methods to the ancestor that defines them,
/// while its own methods override. `B much A` inherits `foo` (defined by class 0)
/// and overrides `bar`.
#[test]
fn inherited_methods_dispatch_to_the_defining_class() {
    let src = "many A:\n    such foo:\n        return 1\n    wow\n    such bar:\n        return 2\n    wow\nwow\nmany B much A:\n    such bar:\n        return 3\n    wow\nwow\nsuch b = B()\nbark b.foo()\nbark b.bar()\nwow\n";
    let out = gen(src).unwrap();
    // B (class 1) inherits foo from A (class 0), overrides bar with its own mf_1_bar.
    assert!(
        out.contains("(1u32, \"foo\") => {"),
        "B dispatches foo:\n{out}"
    );
    assert!(
        out.contains("mf_0_foo(recv"),
        "B's foo calls A's wrapper mf_0_foo:\n{out}"
    );
    assert!(
        out.contains("(1u32, \"bar\") => {"),
        "B dispatches bar:\n{out}"
    );
    assert!(
        out.contains("mf_1_bar(recv"),
        "B's bar overrides with mf_1_bar:\n{out}"
    );
}

/// A child with no `init` of its own constructs through its parent's `init`.
#[test]
fn child_inherits_the_parent_init() {
    let src = "many A:\n    such init much name:\n        self.name = name\n    wow\nwow\nmany B much A:\n    such greet:\n        return self.name\n    wow\nwow\nsuch b = B(\"kabosu\")\nwow\n";
    let out = gen(src).unwrap();
    // n_1 (B's constructor) runs A's init wrapper, mf_0_init, on the new object.
    assert!(
        out.contains("fn n_1(v_name: Value, env: &mut Env)"),
        "B's constructor takes A's init parameters:\n{out}"
    );
    assert!(
        out.contains("mf_0_init(obj.clone()"),
        "B's constructor runs the inherited init:\n{out}"
    );
    // A wrong argument count is checked against the inherited init.
    let err = gen("many A:\n    such init much name:\n        self.name = name\n    wow\nwow\nmany B much A:\n    such greet:\n        return self.name\n    wow\nwow\nsuch b = B()\nwow\n").unwrap_err();
    assert_eq!(err.message, "B takes 1 argument, got 0");
}

/// `super.method(args)` resolves statically to the nearest ancestor that defines
/// the method, called with the current `self` as the receiver.
#[test]
fn super_call_targets_the_parent_wrapper() {
    let src = "many A:\n    such speak:\n        return 1\n    wow\nwow\nmany B much A:\n    such speak:\n        return super.speak()\n    wow\nwow\nsuch b = B()\nbark b.speak()\nwow\n";
    let out = gen(src).unwrap();
    assert!(
        out.contains("mf_0_speak(v_self.clone(), &mut *env)"),
        "super.speak() in B calls A's wrapper with self:\n{out}"
    );
}

/// `super` outside a method (or in a class without a parent) is caught by the
/// checker; codegen never reaches it in a checked program. Reaching codegen
/// directly still errors rather than panicking.
#[test]
fn super_without_a_parent_class_is_a_codegen_error() {
    let err =
        gen("many A:\n    such go:\n        return super.foo()\n    wow\nwow\nsuch a = A()\nwow\n")
            .unwrap_err();
    assert_eq!(err.headline, "very super. much orphan.");
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

/// Build a two-file program by hand (no filesystem) to lock the module
/// name-mangling: a module's function is `f1_…`, its constant is a `g1_…`
/// `Env` field, and a multi-file program carries a `FILES` table.
#[test]
fn module_names_are_mangled_by_file_id() {
    use crate::modules::{Program, ProgramFile};
    let m_src = "so K = 7\nsuch sq much x:\n    return x * x\nwow\nwow\n";
    let e_src = "so m\nbark m.sq(3)\nbark m.K\nwow\n";
    let m = parse("m.doge", m_src).unwrap();
    let e = parse("app.doge", e_src).unwrap();
    let program = Program {
        files: vec![
            ProgramFile {
                file_id: 0,
                is_entry: true,
                name: "app".into(),
                path: "app.doge".into(),
                source: e_src.into(),
                script: e,
                stdlib_imports: vec![],
                user_imports: vec![("m".into(), 1)],
            },
            ProgramFile {
                file_id: 1,
                is_entry: false,
                name: "m".into(),
                path: "m.doge".into(),
                source: m_src.into(),
                script: m,
                stdlib_imports: vec![],
                user_imports: vec![],
            },
        ],
        init_order: vec![1],
    };

    let out = generate_program(&program).unwrap();
    assert!(
        out.contains("f_1_sq("),
        "module call is file-id mangled:\n{out}"
    );
    assert!(
        out.contains("env.g1_K"),
        "module const is a g1_ field:\n{out}"
    );
    assert!(
        out.contains("static FILES"),
        "multi-file uses a FILES table"
    );
    assert!(out.contains("env.cur_file ="), "multi-file tracks cur_file");
}

/// A caught error binds a structured value whose location is the raise site. A
/// single-file program embeds its one path; a multi-file program reads the path
/// from the `FILES` table by the runtime `cur_file`.
#[test]
fn try_in_multifile_reads_the_file_from_the_files_table() {
    use crate::modules::{Program, ProgramFile};
    let e_src = "so m\npls\n    such x = 1 // 0\noh no err!\n    bark err\nwow\n";
    let m_src = "so K = 7\nwow\n";
    let e = parse("app.doge", e_src).unwrap();
    let m = parse("m.doge", m_src).unwrap();
    let program = Program {
        files: vec![
            ProgramFile {
                file_id: 0,
                is_entry: true,
                name: "app".into(),
                path: "app.doge".into(),
                source: e_src.into(),
                script: e,
                stdlib_imports: vec![],
                user_imports: vec![("m".into(), 1)],
            },
            ProgramFile {
                file_id: 1,
                is_entry: false,
                name: "m".into(),
                path: "m.doge".into(),
                source: m_src.into(),
                script: m,
                stdlib_imports: vec![],
                user_imports: vec![],
            },
        ],
        init_order: vec![1],
    };

    let out = generate_program(&program).unwrap();
    assert!(
        out.contains("error_value(&e, FILES[env.cur_file as usize].0, env.cur_line)"),
        "multi-file catch reads the file from the FILES table:\n{out}"
    );
}

/// Objects from every file share one program-wide class-id space, and a module's
/// class is constructed by member. Two files may each define a `Shibe`: the
/// entry's is class 0, the module's is class 1, and `m.Shibe()` builds the latter.
#[test]
fn module_objects_get_global_class_ids() {
    use crate::modules::{Program, ProgramFile};
    let e_src =
        "so m\nmany Shibe:\n    such woof:\n        return 1\n    wow\nwow\nsuch a = Shibe()\nsuch b = m.Shibe()\nbark a.woof()\nbark b.bork_it()\nwow\n";
    let m_src = "many Shibe:\n    such bork_it:\n        return 2\n    wow\nwow\nwow\n";
    let e = parse("app.doge", e_src).unwrap();
    let m = parse("m.doge", m_src).unwrap();
    let program = Program {
        files: vec![
            ProgramFile {
                file_id: 0,
                is_entry: true,
                name: "app".into(),
                path: "app.doge".into(),
                source: e_src.into(),
                script: e,
                stdlib_imports: vec![],
                user_imports: vec![("m".into(), 1)],
            },
            ProgramFile {
                file_id: 1,
                is_entry: false,
                name: "m".into(),
                path: "m.doge".into(),
                source: m_src.into(),
                script: m,
                stdlib_imports: vec![],
                user_imports: vec![],
            },
        ],
        init_order: vec![1],
    };

    let out = generate_program(&program).unwrap();
    // The entry's Shibe is class 0, the module's is class 1 — distinct ids.
    assert!(
        out.contains("fn n_0("),
        "entry Shibe constructs via n_0:\n{out}"
    );
    assert!(
        out.contains("fn n_1("),
        "module Shibe constructs via n_1:\n{out}"
    );
    assert!(
        out.contains("fn mf_1_bork_it("),
        "module method is mf_1_:\n{out}"
    );
    // Name resolution is per-file: `Shibe()` in the entry stays n_0, and the
    // module-qualified `m.Shibe()` builds n_1.
    assert!(
        out.contains("Value::object(1u32, \"Shibe\")"),
        "module Shibe tags instances with class id 1:\n{out}"
    );
    // The dispatcher carries an arm for each class's own method.
    assert!(
        out.contains("(0u32, \"woof\")"),
        "class 0 dispatches woof:\n{out}"
    );
    assert!(
        out.contains("(1u32, \"bork_it\")"),
        "class 1 dispatches bork_it:\n{out}"
    );
}
