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
        // Errors are equal when their type, message, and raise site all match.
        (Value::Error(x), Value::Error(y)) => {
            x.kind == y.kind && x.message == y.message && x.file == y.file && x.line == y.line
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
        | (Value::Function(_), _)
        | (Value::Error(_), _) => false,
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

/// Whether `target` is structurally equal to some element of `items`. Shared by
/// the `in` operator and `List.contains` so their membership rule is identical.
pub(crate) fn slice_contains(items: &[Value], target: &Value) -> bool {
    items.iter().any(|element| values_equal(element, target))
}

/// `needle in container`. Python-style: a List tests element membership, a Dict
/// tests key membership, a Str tests substring. Every other container type is a
/// catchable type error. Matched by container variant (not a wildcard) so a new
/// `Value` variant forces its own decision here.
pub fn in_(needle: Value, container: Value) -> DogeResult {
    let found = match &container {
        Value::List(items) => slice_contains(&items.borrow(), &needle),
        // A dict is keyed by Str; a non-Str needle can never be a key, so it is
        // simply absent rather than an error (mirrors cross-type `==`).
        Value::Dict(entries) => match &needle {
            Value::Str(k) => entries.borrow().contains_key(k.as_ref()),
            _ => false,
        },
        Value::Str(haystack) => match &needle {
            Value::Str(sub) => haystack.contains(sub.as_ref()),
            _ => {
                return Err(DogeError::type_error(format!(
                    "can only check if a Str is in a Str, not {}",
                    needle.describe()
                )));
            }
        },
        Value::Int(_)
        | Value::Float(_)
        | Value::Bool(_)
        | Value::None
        | Value::Object(_)
        | Value::Function(_)
        | Value::Error(_) => {
            return Err(DogeError::type_error(format!(
                "in wants a List, Dict, or Str on the right, not {}",
                container.describe()
            )));
        }
    };
    Ok(Value::Bool(found))
}

/// `needle not in container` — the negation of [`in_`], sharing its type rules so
/// `x not in xs` and `not (x in xs)` always agree.
pub fn not_in(needle: Value, container: Value) -> DogeResult {
    match in_(needle, container)? {
        Value::Bool(found) => Ok(Value::Bool(!found)),
        _ => unreachable!("in_ always yields a Bool"),
    }
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
