//! Registration and dispatch of the runtime natives — the always-in-scope
//! builtins (`len`, `str`, `int`, `float`, `range`) and the stdlib module
//! functions (`nerd.*`, `strings.*`). Registration is driven straight from the
//! compiler's `BUILTINS` and `MODULES` tables, so names and arities can never
//! drift; only the runtime-function dispatch below is written by hand, and a test
//! checks every table entry lands on a real arm.

use std::rc::Rc;

use doge_compiler as dc;
use doge_runtime as rt;
use doge_runtime::{function_arity_error, DogeError, DogeResult, Value};

use crate::{Arity, Callable, Interp, Native};

impl Interp {
    /// Register every builtin and stdlib module function as a native callable with
    /// a stable `fn_id`, so it can be called directly or used as a function value.
    pub(crate) fn register_natives(&mut self) {
        for builtin in dc::BUILTINS {
            let arity = if builtin.arities.len() == 2 {
                Arity::OneOrTwo
            } else {
                Arity::Exact(builtin.arities[0])
            };
            let id = self.callables.len();
            self.callables.push(Rc::new(Callable::Native(Native {
                name: builtin.name.to_string(),
                runtime_fn: builtin.runtime_fn,
                arity,
            })));
            self.builtin_ids.insert(builtin.name.to_string(), id);
        }
        for module in dc::MODULES {
            for func in module.funcs {
                let id = self.callables.len();
                self.callables.push(Rc::new(Callable::Native(Native {
                    name: format!("{}.{}", module.name, func.name),
                    runtime_fn: func.runtime_fn,
                    arity: Arity::Exact(func.arity),
                })));
                self.module_fn_ids
                    .insert((module.name.to_string(), func.name.to_string()), id);
            }
        }
    }
}

/// Invoke a native: arity-check with the same wording an indirect call raises,
/// normalize `range`'s one-argument form, then dispatch to its runtime function.
pub(crate) fn call_native(native: &Native, args: Vec<Value>) -> DogeResult<Value> {
    match native.arity {
        Arity::Exact(n) => {
            if args.len() != n {
                return Err(function_arity_error(&native.name, n, Some(n), args.len()));
            }
            call_runtime(native.runtime_fn, &args)
        }
        Arity::OneOrTwo => match args.len() {
            1 => call_runtime(native.runtime_fn, &[Value::Int(0), args[0].clone()]),
            2 => call_runtime(native.runtime_fn, &args),
            got => Err(function_arity_error(&native.name, 1, Some(2), got)),
        },
    }
}

/// Map a runtime-function name to the `doge-runtime` call the compiled program
/// would emit. The argument count is guaranteed by the caller's arity check.
fn call_runtime(runtime_fn: &str, a: &[Value]) -> DogeResult<Value> {
    match runtime_fn {
        "len" => rt::len(&a[0]),
        "to_str" => Ok(rt::to_str(&a[0])),
        "to_int" => rt::to_int(&a[0]),
        "to_float" => rt::to_float(&a[0]),
        "range" => rt::range(&a[0], &a[1]),
        "nerd_abs" => rt::nerd_abs(&a[0]),
        "nerd_sqrt" => rt::nerd_sqrt(&a[0]),
        "nerd_floor" => rt::nerd_floor(&a[0]),
        "nerd_ceil" => rt::nerd_ceil(&a[0]),
        "nerd_round" => rt::nerd_round(&a[0]),
        "nerd_min" => rt::nerd_min(&a[0], &a[1]),
        "nerd_max" => rt::nerd_max(&a[0], &a[1]),
        "nerd_pow" => rt::nerd_pow(&a[0], &a[1]),
        "strings_beeg" => rt::strings_beeg(&a[0]),
        "strings_smoll" => rt::strings_smoll(&a[0]),
        "strings_trim" => rt::strings_trim(&a[0]),
        "strings_split" => rt::strings_split(&a[0], &a[1]),
        "strings_join" => rt::strings_join(&a[0], &a[1]),
        "strings_contains" => rt::strings_contains(&a[0], &a[1]),
        "strings_replace" => rt::strings_replace(&a[0], &a[1], &a[2]),
        other => Err(DogeError::type_error(format!(
            "interp bug: no runtime function {other}"
        ))),
    }
}

/// The value of a stdlib module constant (`nerd.pi`, `nerd.e`), if it names one.
pub(crate) fn module_const(module: &str, member: &str) -> Option<Value> {
    match (module, member) {
        ("nerd", "pi") => Some(Value::Float(std::f64::consts::PI)),
        ("nerd", "e") => Some(Value::Float(std::f64::consts::E)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_covers_every_table_entry() {
        let interp = Interp::new();
        for builtin in dc::BUILTINS {
            assert!(
                interp.builtin_ids.contains_key(builtin.name),
                "builtin {} was not registered",
                builtin.name
            );
        }
        for module in dc::MODULES {
            for func in module.funcs {
                assert!(
                    interp
                        .module_fn_ids
                        .contains_key(&(module.name.to_string(), func.name.to_string())),
                    "{}.{} was not registered",
                    module.name,
                    func.name
                );
            }
        }
    }

    #[test]
    fn every_registered_native_reaches_a_runtime_arm() {
        // Call each native with correctly-counted (if wrongly-typed) arguments: the
        // result may be a type error, but it must never hit the "no runtime
        // function" fallback — that would mean a table entry drifted past dispatch.
        let interp = Interp::new();
        for callable in &interp.callables {
            let Callable::Native(native) = callable.as_ref() else {
                continue;
            };
            let argc = match native.arity {
                Arity::Exact(n) => n,
                Arity::OneOrTwo => 1,
            };
            let args = vec![Value::Int(1); argc];
            if let Err(err) = call_native(native, args) {
                assert!(
                    !err.message.starts_with("interp bug"),
                    "no dispatch arm for runtime fn {}",
                    native.runtime_fn
                );
            }
        }
    }

    #[test]
    fn stdlib_constants_match_the_table() {
        for module in dc::MODULES {
            for (name, _) in module.consts {
                assert!(
                    module_const(module.name, name).is_some(),
                    "missing interpreter value for {}.{name}",
                    module.name
                );
            }
        }
    }
}
