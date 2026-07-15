//! Value operators, split by concern: `arith` (numeric and unary operators),
//! `bits` (bitwise operators), `compare` (equality and ordering), and `index`
//! (container indexing, slicing, and iteration). The generated glue calls these
//! by name, so behaviour lives here.

use bigdecimal::{BigDecimal, ToPrimitive};

use crate::value::Value;

mod arith;
mod bits;
mod compare;
mod index;
#[cfg(test)]
mod tests;

pub use arith::{add, div, floordiv, mul, neg, not_, pow, rem, sub};
pub use bits::{bitand, bitnot, bitor, bitxor, shl, shr};
pub use compare::{eq, ge, gt, in_, le, lt, ne, not_in, values_equal};
pub(crate) use compare::{order, slice_contains};
pub use index::{index_get, index_set, iter_value, slice_get, unpack_value};

/// View a numeric value as `f64` for mixed Int/Float math. `Int` (now an
/// arbitrary-precision `BigInt`) rounds to the nearest `f64`; `Decimal` is
/// deliberately excluded — it is exact and never silently joins inexact Float
/// math (a Float/Decimal arithmetic mix is a type error).
pub(super) fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => n.to_f64(),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// View a value as an exact `BigDecimal` for Int/Decimal math. `Int` promotes
/// exactly; `Decimal` is itself; `Float` is excluded (inexact — see [`as_f64`]).
pub(super) fn as_decimal(v: &Value) -> Option<BigDecimal> {
    match v {
        Value::Int(n) => Some(BigDecimal::from(n.clone())),
        Value::Decimal(d) => Some(d.clone()),
        _ => None,
    }
}

/// Whether `v` is a `Decimal` — used to route an operand pair onto the exact
/// decimal path (and to reject a Float/Decimal mix).
pub(super) fn is_decimal(v: &Value) -> bool {
    matches!(v, Value::Decimal(_))
}

/// Whether `v` is a `Float` — the inexact type that must not silently mix with an
/// exact `Decimal`.
pub(super) fn is_float(v: &Value) -> bool {
    matches!(v, Value::Float(_))
}
