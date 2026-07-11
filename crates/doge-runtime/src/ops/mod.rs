//! Value operators, split by concern: `arith` (numeric and unary operators),
//! `bits` (bitwise operators), `compare` (equality and ordering), and `index`
//! (container indexing, slicing, and iteration). The generated glue calls these
//! by name, so behaviour lives here.

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

/// View a numeric value as `f64` for mixed-type math. Non-numeric values are not
/// numbers, so they have no `f64` view.
pub(super) fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}
