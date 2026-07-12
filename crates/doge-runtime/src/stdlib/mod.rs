pub mod env;
pub mod fetch;
pub mod nerd;
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
