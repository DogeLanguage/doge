//! Value operators, split by concern: `arith` (numeric and unary operators),
//! `compare` (equality and ordering), and `index` (container indexing and
//! iteration). The generated glue calls these by name, so behaviour lives here.

use crate::value::Value;

mod arith;
mod compare;
mod index;
#[cfg(test)]
mod tests;

pub use arith::{add, div, floordiv, mul, neg, not_, rem, sub};
pub(crate) use compare::order;
pub use compare::{eq, ge, gt, le, lt, ne, values_equal};
pub use index::{index_get, index_set, iter_value};

/// View a numeric value as `f64` for mixed-type math. Non-numeric values are not
/// numbers, so they have no `f64` view.
pub(super) fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}
