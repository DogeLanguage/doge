use std::cmp::Ordering;
use std::rc::Rc;

use bigdecimal::{BigDecimal, ToPrimitive};

use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// Whether `v` is one of the three numeric types (`Int`/`Float`/`Decimal`), which
/// compare across each other rather than by exact type.
fn is_numeric(v: &Value) -> bool {
    matches!(v, Value::Int(_) | Value::Float(_) | Value::Decimal(_))
}

/// A numeric value as `f64`, for any comparison that pairs a `Float` with an
/// `Int`/`Decimal`. `None` only for a non-number or a `Float` NaN.
fn numeric_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => n.to_f64(),
        Value::Float(f) => Some(*f),
        Value::Decimal(d) => d.to_f64(),
        _ => None,
    }
}

/// Ordering between two numeric values, comparing exactly where both are exact
/// (Int/Decimal) and via `f64` once a `Float` is involved. `None` when a NaN is
/// involved (no ordering). Both operands must be numeric.
fn numeric_cmp(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::Decimal(x), Value::Decimal(y)) => Some(x.cmp(y)),
        (Value::Int(x), Value::Decimal(y)) => Some(BigDecimal::from(x.clone()).cmp(y)),
        (Value::Decimal(x), Value::Int(y)) => Some(x.cmp(&BigDecimal::from(y.clone()))),
        _ => numeric_f64(a)?.partial_cmp(&numeric_f64(b)?),
    }
}

/// Numeric equality when both operands are numbers (`Some`), else `None` so the
/// caller falls through to structural/type comparison. Uses value ordering, so
/// `dec("0.10") == dec("0.1")` and `1 == 1.0` both hold.
fn numeric_equal(a: &Value, b: &Value) -> Option<bool> {
    if is_numeric(a) && is_numeric(b) {
        Some(numeric_cmp(a, b) == Some(Ordering::Equal))
    } else {
        None
    }
}

/// Structural, Python-style equality: `1 == 1.0`, deep list/dict comparison,
/// everything else across types is unequal.
pub fn values_equal(a: &Value, b: &Value) -> bool {
    if let Some(numeric) = numeric_equal(a, b) {
        return numeric;
    }
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Bytes(x), Value::Bytes(y)) => x == y,
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
        // Function equality includes captured-cell identity, not just its definition.
        (Value::Function(x), Value::Function(y)) => {
            x.fn_id == y.fn_id
                && x.captures.len() == y.captures.len()
                && x.captures
                    .iter()
                    .zip(y.captures.iter())
                    .all(|(p, q)| Rc::ptr_eq(p, q))
        }
        // Classes have no captures, so constructor identity is sufficient.
        (Value::Class(x), Value::Class(y)) => x.fn_id == y.fn_id,
        // Bound methods include receiver identity.
        (Value::BoundMethod(x), Value::BoundMethod(y)) => {
            x.method == y.method && values_equal(&x.receiver, &y.receiver)
        }
        // Errors are equal when their type, message, and raise site all match.
        (Value::Error(x), Value::Error(y)) => {
            x.kind == y.kind && x.message == y.message && x.file == y.file && x.line == y.line
        }
        // Sockets are equal only when they are the very same handle.
        (Value::Socket(x), Value::Socket(y)) => Rc::ptr_eq(x, y),
        // Pups and bowls, like sockets, are equal only to the very same handle.
        (Value::Pup(x), Value::Pup(y)) => Rc::ptr_eq(x, y),
        (Value::Bowl(x), Value::Bowl(y)) => Rc::ptr_eq(x, y),
        // Explicit variants make a new same-type equality case mandatory.
        (Value::Int(_), _)
        | (Value::Float(_), _)
        | (Value::Decimal(_), _)
        | (Value::Str(_), _)
        | (Value::Bytes(_), _)
        | (Value::Bool(_), _)
        | (Value::None, _)
        | (Value::List(_), _)
        | (Value::Dict(_), _)
        | (Value::Object(_), _)
        | (Value::Function(_), _)
        | (Value::Class(_), _)
        | (Value::BoundMethod(_), _)
        | (Value::Error(_), _)
        | (Value::Socket(_), _)
        | (Value::Pup(_), _)
        | (Value::Bowl(_), _) => false,
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
        // A non-Str cannot be a Dict key, so it is absent rather than an error.
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
        // Int tests byte membership; Bytes tests a contiguous sub-slice.
        Value::Bytes(haystack) => match &needle {
            Value::Int(n) => n.to_u8().is_some_and(|b| haystack.contains(&b)),
            Value::Bytes(sub) => {
                sub.is_empty() || haystack.windows(sub.len()).any(|w| w == &sub[..])
            }
            _ => {
                return Err(DogeError::type_error(format!(
                    "can only check if an Int or Bytes is in a Bytes, not {}",
                    needle.describe()
                )));
            }
        },
        Value::Int(_)
        | Value::Float(_)
        | Value::Decimal(_)
        | Value::Bool(_)
        | Value::None
        | Value::Object(_)
        | Value::Function(_)
        | Value::Class(_)
        | Value::BoundMethod(_)
        | Value::Error(_)
        | Value::Socket(_)
        | Value::Pup(_)
        | Value::Bowl(_) => {
            return Err(DogeError::type_error(format!(
                "in wants a List, Dict, Str, or Bytes on the right, not {}",
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
    if is_numeric(a) && is_numeric(b) {
        return numeric_cmp(a, b).ok_or_else(|| {
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
    if let (Value::Bytes(x), Value::Bytes(y)) = (a, b) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_membership_subslice_and_ordering() {
        let hay = Value::bytes([104, 105, 33]);
        // `int in bytes` is byte membership; a value outside 0..=255 is simply absent.
        assert!(matches!(
            in_(Value::int(105), hay.clone()).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            in_(Value::int(200), hay.clone()).unwrap(),
            Value::Bool(false)
        ));
        assert!(matches!(
            in_(Value::int(999), hay.clone()).unwrap(),
            Value::Bool(false)
        ));
        // `bytes in bytes` is a contiguous sub-slice.
        assert!(matches!(
            in_(Value::bytes([105, 33]), hay.clone()).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            in_(Value::bytes([104, 33]), hay.clone()).unwrap(),
            Value::Bool(false)
        ));
        // Ordering is byte-wise.
        assert_eq!(
            order(&Value::bytes([1, 2]), &Value::bytes([1, 3])).unwrap(),
            Ordering::Less
        );
    }

    #[test]
    fn bound_methods_equal_only_on_the_same_receiver_and_name() {
        let a = Value::object(0, "Shibe");
        let b = Value::object(0, "Shibe");
        // Same instance, same method → equal.
        assert!(values_equal(
            &Value::bound_method(a.clone(), "speak"),
            &Value::bound_method(a.clone(), "speak"),
        ));
        // Same instance, different method → not equal.
        assert!(!values_equal(
            &Value::bound_method(a.clone(), "speak"),
            &Value::bound_method(a.clone(), "wag"),
        ));
        // Different instances of the same class → not equal.
        assert!(!values_equal(
            &Value::bound_method(a, "speak"),
            &Value::bound_method(b, "speak"),
        ));
    }
}
