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
            let arity = match builtin.shape {
                dc::BuiltinShape::Range => Arity::OneOrTwo,
                dc::BuiltinShape::Prompt => Arity::ZeroOrOne,
                _ => Arity::Exact(builtin.arities[0]),
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
        Arity::ZeroOrOne => match args.len() {
            0 | 1 => call_runtime(native.runtime_fn, &args),
            got => Err(function_arity_error(&native.name, 0, Some(1), got)),
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
        "gib" => rt::gib(a.first()),
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
        "fetch_read" => rt::fetch_read(&a[0]),
        "fetch_write" => rt::fetch_write(&a[0], &a[1]),
        "fetch_append" => rt::fetch_append(&a[0], &a[1]),
        "fetch_exists" => rt::fetch_exists(&a[0]),
        "fetch_delete" => rt::fetch_delete(&a[0]),
        "env_args" => rt::env_args(),
        "env_get" => rt::env_get(&a[0]),
        "howl_listen" => rt::howl_listen(&a[0], &a[1]),
        "howl_connect" => rt::howl_connect(&a[0], &a[1]),
        "howl_accept" => rt::howl_accept(&a[0]),
        "howl_port" => rt::howl_port(&a[0]),
        "howl_send" => rt::howl_send(&a[0], &a[1]),
        "howl_recv" => rt::howl_recv(&a[0], &a[1]),
        "howl_recv_line" => rt::howl_recv_line(&a[0]),
        "howl_close" => rt::howl_close(&a[0]),
        "howl_get" => rt::howl_get(&a[0]),
        "howl_post" => rt::howl_post(&a[0], &a[1]),
        "json_parse" => rt::json_parse(&a[0]),
        "json_emit" => rt::json_emit(&a[0]),
        "dson_parse" => rt::dson_parse(&a[0]),
        "dson_emit" => rt::dson_emit(&a[0]),
        "pack_fetch" => rt::pack_fetch(&a[0]),
        "pack_bowl" => rt::pack_bowl(),
        "pack_drop" => rt::pack_drop(&a[0], &a[1]),
        "pack_sniff" => rt::pack_sniff(&a[0]),
        // `pack.zoom` rebuilds an interpreter on a new thread, which needs
        // interpreter state, so it is dispatched in `call_id` (see `interp_zoom`),
        // never here. This arm keeps the "every native reaches a runtime arm"
        // parity invariant honest for the one native that is special.
        dc::PACK_ZOOM_RUNTIME_FN => Err(DogeError::type_error(
            "pack.zoom cannot run as a bare native",
        )),
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
                // One argument keeps `gib` from reading stdin: the wrongly-typed
                // prompt errors out before any read.
                Arity::ZeroOrOne => 1,
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
