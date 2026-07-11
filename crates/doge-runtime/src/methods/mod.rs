//! Built-in method dispatch for collection values — the counterpart of
//! [`crate::objects`] for `many` instances. A method call on a non-`Object`
//! receiver (`xs.append(1)`, `d.keys()`) routes here; the generated `call_method`
//! dispatcher handles `Object` receivers itself and forwards everything else.
//!
//! Behaviour lives here, not in codegen (generated Rust is thin glue): each
//! method mutates or reads the value's shared cell and returns a `Value`, and
//! every failure is a catchable [`DogeError`], never a panic.

use std::rc::Rc;

use crate::error::{DogeError, DogeResult};
use crate::objects::method_arity_error;
use crate::value::Value;

mod dict;
mod list;
#[cfg(test)]
mod tests;

use dict::dict_method;
use list::list_method;

/// Dispatch a method call on a List or Dict. `recv` is the receiver, `name` the
/// method, `args` the already-evaluated arguments. A non-collection receiver, an
/// unknown method, or a wrong argument count is a catchable error.
pub fn builtin_method(recv: &Value, name: &str, args: Vec<Value>) -> DogeResult {
    match recv {
        Value::List(_) => list_method(recv, name, args),
        Value::Dict(_) => dict_method(recv, name, args),
        // The dispatcher routes objects to its own class match, never here; this
        // defensive branch names the class rather than claiming "no methods".
        Value::Object(_) => Err(crate::objects::no_such_method(recv, name)),
        // No other value has methods at all. Listed by variant rather than a
        // wildcard, so a new Value variant with methods forces a decision here.
        Value::Int(_)
        | Value::Float(_)
        | Value::Str(_)
        | Value::Bool(_)
        | Value::None
        | Value::Function(_)
        | Value::Error(_) => Err(crate::objects::no_methods_error(recv)),
    }
}

/// The arity gate every method runs first: reuses the object-method wording so a
/// List/Dict arity error reads exactly like `Shibe.speak takes 1 argument, got 0`.
pub(super) fn check_arity(
    class: &str,
    method: &str,
    expected: usize,
    got: usize,
) -> DogeResult<()> {
    if got == expected {
        Ok(())
    } else {
        Err(method_arity_error(
            class,
            method,
            expected,
            Some(expected),
            got,
        ))
    }
}

/// Take an argument that must be an Int, or raise the standard type error naming
/// the method and what it got. `what` is the argument's role, e.g. `List.insert
/// needs an Int index`.
pub(super) fn expect_int(value: Value, what: &str) -> DogeResult<i64> {
    match value {
        Value::Int(n) => Ok(n),
        other => Err(DogeError::type_error(format!(
            "{what}, got {}",
            other.describe()
        ))),
    }
}

/// Take an argument that must be a Str, or raise the standard type error naming
/// the method and what it got.
pub(super) fn expect_str(value: Value, what: &str) -> DogeResult<Rc<str>> {
    match value {
        Value::Str(s) => Ok(s),
        other => Err(DogeError::type_error(format!(
            "{what}, got {}",
            other.describe()
        ))),
    }
}
