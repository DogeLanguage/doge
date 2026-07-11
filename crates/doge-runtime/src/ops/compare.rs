use std::cmp::Ordering;
use std::rc::Rc;

use super::as_f64;
use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// Structural, Python-style equality: `1 == 1.0`, deep list/dict comparison,
/// everything else across types is unequal.
pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) => (*x as f64) == *y,
        (Value::Float(x), Value::Int(y)) => *x == (*y as f64),
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::None, Value::None) => true,
        (Value::List(x), Value::List(y)) => {
            let (xb, yb) = (x.borrow(), y.borrow());
            xb.len() == yb.len() && xb.iter().zip(yb.iter()).all(|(p, q)| values_equal(p, q))
        }
        (Value::Dict(x), Value::Dict(y)) => {
            let (xb, yb) = (x.borrow(), y.borrow());
            xb.len() == yb.len()
                && xb
                    .iter()
                    .all(|(k, v)| yb.get(k).is_some_and(|w| values_equal(v, w)))
        }
        // Objects are equal only when they are the very same instance.
        (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(x, y),
        // Functions are equal when they share a definition and the very same
        // captured cells: `greet == greet` holds, but two closures built from the
        // same definition over different environments do not.
        (Value::Function(x), Value::Function(y)) => {
            x.fn_id == y.fn_id
                && x.captures.len() == y.captures.len()
                && x.captures
                    .iter()
                    .zip(y.captures.iter())
                    .all(|(p, q)| Rc::ptr_eq(p, q))
        }
        // Cross-type comparisons are simply unequal, never an error. Written by
        // left-hand variant rather than a wildcard, so a new Value variant forces
        // its own same-type case to be added above.
        (Value::Int(_), _)
        | (Value::Float(_), _)
        | (Value::Str(_), _)
        | (Value::Bool(_), _)
        | (Value::None, _)
        | (Value::List(_), _)
        | (Value::Dict(_), _)
        | (Value::Object(_), _)
        | (Value::Function(_), _) => false,
    }
}

/// `==`.
pub fn eq(a: Value, b: Value) -> DogeResult {
    Ok(Value::Bool(values_equal(&a, &b)))
}

/// `!=`.
pub fn ne(a: Value, b: Value) -> DogeResult {
    Ok(Value::Bool(!values_equal(&a, &b)))
}

/// Ordering for `< <= > >=`: numbers compare across Int/Float, Str compares
/// lexicographically, anything else is a type error. The list `sort` method
/// reuses this so its ordering matches the comparison operators exactly.
pub(crate) fn order(a: &Value, b: &Value) -> DogeResult<Ordering> {
    if let (Some(x), Some(y)) = (as_f64(a), as_f64(b)) {
        return x.partial_cmp(&y).ok_or_else(|| {
            DogeError::type_error(format!(
                "cannot compare {} and {}",
                a.describe(),
                b.describe()
            ))
        });
    }
    if let (Value::Str(x), Value::Str(y)) = (a, b) {
        return Ok(x.as_ref().cmp(y.as_ref()));
    }
    Err(DogeError::type_error(format!(
        "cannot compare {} and {}",
        a.describe(),
        b.describe()
    )))
}

/// `<`.
pub fn lt(a: Value, b: Value) -> DogeResult {
    Ok(Value::Bool(order(&a, &b)? == Ordering::Less))
}

/// `<=`.
pub fn le(a: Value, b: Value) -> DogeResult {
    Ok(Value::Bool(matches!(
        order(&a, &b)?,
        Ordering::Less | Ordering::Equal
    )))
}

/// `>`.
pub fn gt(a: Value, b: Value) -> DogeResult {
    Ok(Value::Bool(order(&a, &b)? == Ordering::Greater))
}

/// `>=`.
pub fn ge(a: Value, b: Value) -> DogeResult {
    Ok(Value::Bool(matches!(
        order(&a, &b)?,
        Ordering::Greater | Ordering::Equal
    )))
}
