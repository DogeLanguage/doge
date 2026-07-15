pub mod chase;
pub mod dson;
pub mod env;
pub mod fetch;
pub mod howl;
pub mod hunt;
pub mod json;
pub mod nap;
pub mod nerd;
pub mod pack;
pub mod roll;
mod serialize;
pub mod strings;

use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// A Str argument as `&str`, or a catchable type error naming the module member
/// (`{module}.{fname}`). Shared by every stdlib module that takes a Str argument.
pub(crate) fn str_arg<'a>(module: &str, fname: &str, v: &'a Value) -> DogeResult<&'a str> {
    match v {
        Value::Str(s) => Ok(s),
        _ => Err(DogeError::type_error(format!(
            "{module}.{fname} needs a Str, got {}",
            v.describe()
        ))),
    }
}

/// A Bytes argument as `&[u8]`, or a catchable type error naming the module
/// member. Shared by every stdlib module that takes a Bytes argument.
pub(crate) fn bytes_arg<'a>(module: &str, fname: &str, v: &'a Value) -> DogeResult<&'a [u8]> {
    match v {
        Value::Bytes(b) => Ok(b),
        _ => Err(DogeError::type_error(format!(
            "{module}.{fname} needs a Bytes, got {}",
            v.describe()
        ))),
    }
}

/// An Int argument as `i64`, or a catchable type error naming the module member.
/// Shared by every stdlib module that takes an Int argument.
pub(crate) fn int_arg(module: &str, fname: &str, v: &Value) -> DogeResult<i64> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => Err(DogeError::type_error(format!(
            "{module}.{fname} needs an Int, got {}",
            v.describe()
        ))),
    }
}
