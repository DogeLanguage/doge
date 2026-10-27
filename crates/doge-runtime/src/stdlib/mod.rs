pub mod chase;
pub mod crypto;
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

use std::time::Duration;

use bigdecimal::ToPrimitive;

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
        Value::Int(n) => n
            .to_i64()
            .ok_or_else(|| DogeError::value_error(format!("{module}.{fname}: {n} is too large"))),
        _ => Err(DogeError::type_error(format!(
            "{module}.{fname} needs an Int, got {}",
            v.describe()
        ))),
    }
}

/// A duration argument (Int or Float seconds) as a `Duration`, or a catchable
/// error naming the module member: a non-number is a `TypeError`; a negative,
/// non-finite, or out-of-range count is a `ValueError` rather than the panic
/// `Duration::from_secs_f64` would raise. Shared by every member that waits.
pub(crate) fn seconds_arg(module: &str, fname: &str, v: &Value) -> DogeResult<Duration> {
    let secs = match v {
        Value::Int(n) => n
            .to_f64()
            .ok_or_else(|| DogeError::overflow(format!("{n} is too large for {module}.{fname}")))?,
        Value::Float(f) => *f,
        _ => {
            return Err(DogeError::type_error(format!(
                "{module}.{fname} needs a number of seconds, got {}",
                v.describe()
            )))
        }
    };
    if !secs.is_finite() || secs < 0.0 {
        return Err(DogeError::value_error(format!(
            "{module}.{fname} cannot wait {secs} seconds — the duration must be finite and not negative"
        )));
    }
    Duration::try_from_secs_f64(secs).map_err(|_| {
        DogeError::value_error(format!(
            "{module}.{fname} cannot wait that long — the duration is out of range"
        ))
    })
}
