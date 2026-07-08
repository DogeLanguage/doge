use std::cmp::Ordering;

use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// View a numeric value as `f64` for mixed-type math.
fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn type_err_binop(sym: &str, a: &Value, b: &Value) -> DogeError {
    DogeError::type_error(format!(
        "cannot {sym} {} and {}",
        a.describe(),
        b.describe()
    ))
}

fn overflow(sym: &str, x: i64, y: i64) -> DogeError {
    DogeError::overflow(format!("{x} {sym} {y} overflowed the Int range"))
}

fn div_by_zero(sym: &str) -> DogeError {
    DogeError::division_by_zero(format!("cannot {sym} by zero"))
}

/// Numeric fallback shared by `sub`/`mul`: promote both operands to Float and
/// apply `op`, or raise a type error if either operand is non-numeric.
fn float_fallback(sym: &str, a: &Value, b: &Value, op: impl Fn(f64, f64) -> f64) -> DogeResult {
    match (as_f64(a), as_f64(b)) {
        (Some(x), Some(y)) => Ok(Value::Float(op(x, y))),
        _ => Err(type_err_binop(sym, a, b)),
    }
}

/// `+` — Int+Int (checked), Float promotion, Str concatenation, List concatenation.
pub fn add(a: Value, b: Value) -> DogeResult {
    match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => x
            .checked_add(*y)
            .map(Value::Int)
            .ok_or_else(|| overflow("+", *x, *y)),
        (Value::Str(x), Value::Str(y)) => Ok(Value::str(format!("{x}{y}"))),
        (Value::List(x), Value::List(y)) => {
            let mut joined = x.borrow().clone();
            joined.extend(y.borrow().iter().cloned());
            Ok(Value::list(joined))
        }
        _ => match (as_f64(&a), as_f64(&b)) {
            (Some(x), Some(y)) => Ok(Value::Float(x + y)),
            _ => Err(type_err_binop("+", &a, &b)),
        },
    }
}

/// `-` — Int-Int (checked) or Float promotion.
pub fn sub(a: Value, b: Value) -> DogeResult {
    match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => x
            .checked_sub(*y)
            .map(Value::Int)
            .ok_or_else(|| overflow("-", *x, *y)),
        _ => float_fallback("-", &a, &b, |x, y| x - y),
    }
}

/// `*` — Int*Int (checked) or Float promotion.
pub fn mul(a: Value, b: Value) -> DogeResult {
    match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => x
            .checked_mul(*y)
            .map(Value::Int)
            .ok_or_else(|| overflow("*", *x, *y)),
        _ => float_fallback("*", &a, &b, |x, y| x * y),
    }
}

/// `/` — always returns a Float (`5 / 2 == 2.5`), per DESIGN §2.
pub fn div(a: Value, b: Value) -> DogeResult {
    match (as_f64(&a), as_f64(&b)) {
        (Some(_), Some(0.0)) => Err(div_by_zero("/")),
        (Some(x), Some(y)) => Ok(Value::Float(x / y)),
        _ => Err(type_err_binop("/", &a, &b)),
    }
}

/// `//` — floor division. Int//Int yields an Int (floored toward negative
/// infinity, Python-style); any Float operand yields a floored Float.
pub fn floordiv(a: Value, b: Value) -> DogeResult {
    match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => {
            if *y == 0 {
                return Err(div_by_zero("//"));
            }
            let q = x.checked_div(*y).ok_or_else(|| overflow("//", *x, *y))?;
            let r = x % y;
            // Truncated division rounds toward zero; nudge down one when the
            // remainder is non-zero and operands have opposite signs.
            let floored = if r != 0 && ((r < 0) != (*y < 0)) {
                q - 1
            } else {
                q
            };
            Ok(Value::Int(floored))
        }
        _ => match (as_f64(&a), as_f64(&b)) {
            (Some(_), Some(0.0)) => Err(div_by_zero("//")),
            (Some(x), Some(y)) => Ok(Value::Float((x / y).floor())),
            _ => Err(type_err_binop("//", &a, &b)),
        },
    }
}

/// `%` — modulo whose result takes the sign of the divisor (Python-style), so
/// that `a == (a // b) * b + (a % b)` holds.
pub fn rem(a: Value, b: Value) -> DogeResult {
    match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => {
            if *y == 0 {
                return Err(div_by_zero("%"));
            }
            let r = x.checked_rem(*y).ok_or_else(|| overflow("%", *x, *y))?;
            let m = if r != 0 && ((r < 0) != (*y < 0)) {
                r + y
            } else {
                r
            };
            Ok(Value::Int(m))
        }
        _ => match (as_f64(&a), as_f64(&b)) {
            (Some(_), Some(0.0)) => Err(div_by_zero("%")),
            (Some(x), Some(y)) => {
                let r = x % y;
                let m = if r != 0.0 && ((r < 0.0) != (y < 0.0)) {
                    r + y
                } else {
                    r
                };
                Ok(Value::Float(m))
            }
            _ => Err(type_err_binop("%", &a, &b)),
        },
    }
}

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
        _ => false,
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
/// lexicographically, anything else is a type error.
fn order(a: &Value, b: &Value) -> DogeResult<Ordering> {
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

/// Unary `-`.
pub fn neg(a: Value) -> DogeResult {
    match &a {
        Value::Int(n) => n
            .checked_neg()
            .map(Value::Int)
            .ok_or_else(|| DogeError::overflow(format!("-{n} overflowed the Int range"))),
        Value::Float(f) => Ok(Value::Float(-f)),
        _ => Err(DogeError::type_error(format!(
            "cannot negate {}",
            a.describe()
        ))),
    }
}

/// `not` — always succeeds, using Python truthiness.
pub fn not_(a: Value) -> DogeResult {
    Ok(Value::Bool(!a.truthy()))
}

/// Resolve a possibly-negative index against a length, or raise a catchable
/// out-of-bounds error. Negative indices count from the end (`-1` is the last).
fn normalize_index(i: i64, len: usize) -> DogeResult<usize> {
    let len_i = len as i64;
    let idx = if i < 0 { i + len_i } else { i };
    if idx < 0 || idx >= len_i {
        Err(DogeError::index_out_of_bounds(format!(
            "index {i} is out of bounds for length {len}"
        )))
    } else {
        Ok(idx as usize)
    }
}

/// `container[index]`. List by Int, Dict by Str key, Str by character index
/// (never byte index — `"héllo"[1] == "é"`).
pub fn index_get(container: &Value, index: &Value) -> DogeResult {
    match (container, index) {
        (Value::List(items), Value::Int(i)) => {
            let items = items.borrow();
            let idx = normalize_index(*i, items.len())?;
            Ok(items[idx].clone())
        }
        (Value::Str(s), Value::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            let idx = normalize_index(*i, chars.len())?;
            Ok(Value::str(chars[idx].to_string()))
        }
        (Value::Dict(d), Value::Str(k)) => d
            .borrow()
            .get(k.as_ref())
            .cloned()
            .ok_or_else(|| DogeError::key_error(format!("no such key: {k:?}"))),
        (Value::List(_) | Value::Str(_), _) => Err(DogeError::type_error(format!(
            "cannot index {} with {} (need an Int)",
            container.describe(),
            index.describe()
        ))),
        (Value::Dict(_), _) => Err(DogeError::type_error(format!(
            "cannot index a Dict with {} (keys are Str)",
            index.describe()
        ))),
        _ => Err(DogeError::type_error(format!(
            "cannot index {}",
            container.describe()
        ))),
    }
}

/// `container[index] = value`. List and Dict are mutable in place; Str is
/// immutable, so assigning into one is a catchable type error.
pub fn index_set(container: &Value, index: &Value, value: Value) -> DogeResult<()> {
    match (container, index) {
        (Value::List(items), Value::Int(i)) => {
            let mut items = items.borrow_mut();
            let idx = normalize_index(*i, items.len())?;
            items[idx] = value;
            Ok(())
        }
        (Value::Dict(d), Value::Str(k)) => {
            d.borrow_mut().insert(k.to_string(), value);
            Ok(())
        }
        (Value::Str(_), _) => Err(DogeError::type_error(
            "cannot assign into a Str (Str values are immutable)",
        )),
        (Value::List(_), _) => Err(DogeError::type_error(format!(
            "cannot index a List with {} (need an Int)",
            index.describe()
        ))),
        (Value::Dict(_), _) => Err(DogeError::type_error(format!(
            "cannot index a Dict with {} (keys are Str)",
            index.describe()
        ))),
        _ => Err(DogeError::type_error(format!(
            "cannot index into {}",
            container.describe()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn int(n: i64) -> Value {
        Value::Int(n)
    }
    fn float(f: f64) -> Value {
        Value::Float(f)
    }

    #[test]
    fn division_always_float() {
        // 5 / 2 is 2.5, never 2 — the whole reason `/` differs from `//`.
        assert!(matches!(div(int(5), int(2)).unwrap(), Value::Float(f) if f == 2.5));
        assert!(matches!(div(int(4), int(2)).unwrap(), Value::Float(f) if f == 2.0));
    }

    #[test]
    fn floordiv_is_integer_division() {
        assert!(matches!(floordiv(int(5), int(2)).unwrap(), Value::Int(2)));
        // Floors toward negative infinity, Python-style, not toward zero.
        assert!(matches!(floordiv(int(-7), int(2)).unwrap(), Value::Int(-4)));
        assert!(matches!(floordiv(int(7), int(-2)).unwrap(), Value::Int(-4)));
    }

    #[test]
    fn floordiv_and_rem_are_consistent() {
        // a == (a // b) * b + (a % b) for the tricky negative cases.
        for (a, b) in [(-7, 2), (7, -2), (-7, -2), (7, 2)] {
            let q = match floordiv(int(a), int(b)).unwrap() {
                Value::Int(n) => n,
                _ => unreachable!(),
            };
            let r = match rem(int(a), int(b)).unwrap() {
                Value::Int(n) => n,
                _ => unreachable!(),
            };
            assert_eq!(a, q * b + r, "identity failed for {a} // {b}");
        }
    }

    #[test]
    fn mixed_int_float_promotion() {
        assert!(matches!(add(int(1), float(0.5)).unwrap(), Value::Float(f) if f == 1.5));
        assert!(matches!(mul(float(2.0), int(3)).unwrap(), Value::Float(f) if f == 6.0));
    }

    #[test]
    fn overflow_is_catchable_error() {
        // i64::MAX + 1 is an error a program can catch, never a silent wrap.
        let err = add(int(i64::MAX), int(1)).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Overflow);
    }

    #[test]
    fn division_by_zero_is_catchable() {
        assert_eq!(
            div(int(1), int(0)).unwrap_err().kind,
            ErrorKind::DivisionByZero
        );
        assert_eq!(
            floordiv(int(1), int(0)).unwrap_err().kind,
            ErrorKind::DivisionByZero
        );
        assert_eq!(
            rem(int(1), int(0)).unwrap_err().kind,
            ErrorKind::DivisionByZero
        );
    }

    #[test]
    fn string_and_list_concatenation() {
        assert!(
            matches!(add(Value::str("much "), Value::str("wow")).unwrap(), Value::Str(s) if &*s == "much wow")
        );
        let joined = add(Value::list(vec![int(1)]), Value::list(vec![int(2), int(3)])).unwrap();
        match joined {
            Value::List(items) => assert_eq!(items.borrow().len(), 3),
            _ => panic!("expected a list"),
        }
    }

    #[test]
    fn wrong_type_operands_are_type_errors() {
        let err = add(Value::str("dog"), int(5)).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
        assert_eq!(err.message, "cannot + a Str and an Int");
    }

    #[test]
    fn equality_cross_numeric() {
        assert!(values_equal(&int(1), &float(1.0)));
        assert!(!values_equal(&int(1), &float(1.5)));
        // Different types are simply unequal, never an error.
        assert!(!values_equal(&int(1), &Value::str("1")));
    }

    #[test]
    fn ordering_across_numbers_and_strings() {
        assert!(matches!(lt(int(1), float(1.5)).unwrap(), Value::Bool(true)));
        assert!(matches!(
            gt(Value::str("cheems"), Value::str("bonk")).unwrap(),
            Value::Bool(true)
        ));
        // Comparing across incomparable types is a catchable error.
        assert_eq!(
            lt(int(1), Value::str("x")).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn string_indexing_is_char_based() {
        // Byte indexing would split the two-byte 'é'; Doge indexes characters.
        let hello = Value::str("héllo");
        assert!(matches!(index_get(&hello, &int(1)).unwrap(), Value::Str(s) if &*s == "é"));
        assert!(matches!(index_get(&hello, &int(0)).unwrap(), Value::Str(s) if &*s == "h"));
    }

    #[test]
    fn negative_indices_count_from_the_end() {
        let xs = Value::list(vec![int(10), int(20), int(30)]);
        assert!(matches!(index_get(&xs, &int(-1)).unwrap(), Value::Int(30)));
        assert!(matches!(index_get(&xs, &int(-3)).unwrap(), Value::Int(10)));
    }

    #[test]
    fn oob_index_is_catchable_error() {
        let xs = Value::list(vec![int(1), int(2)]);
        assert_eq!(
            index_get(&xs, &int(5)).unwrap_err().kind,
            ErrorKind::IndexOutOfBounds
        );
        assert_eq!(
            index_get(&xs, &int(-3)).unwrap_err().kind,
            ErrorKind::IndexOutOfBounds
        );
    }

    #[test]
    fn missing_dict_key_is_catchable_error() {
        let mut map = std::collections::HashMap::new();
        map.insert("name".to_string(), Value::str("kabosu"));
        let d = Value::dict(map);
        assert!(
            matches!(index_get(&d, &Value::str("name")).unwrap(), Value::Str(s) if &*s == "kabosu")
        );
        assert_eq!(
            index_get(&d, &Value::str("age")).unwrap_err().kind,
            ErrorKind::KeyError
        );
    }

    #[test]
    fn index_set_mutates_list_and_dict() {
        let xs = Value::list(vec![int(1), int(2)]);
        index_set(&xs, &int(0), int(99)).unwrap();
        assert!(matches!(index_get(&xs, &int(0)).unwrap(), Value::Int(99)));

        let d = Value::dict(std::collections::HashMap::new());
        index_set(&d, &Value::str("k"), int(7)).unwrap();
        assert!(matches!(
            index_get(&d, &Value::str("k")).unwrap(),
            Value::Int(7)
        ));

        // Strings are immutable — a catchable type error, not a panic.
        assert_eq!(
            index_set(&Value::str("dog"), &int(0), Value::str("x"))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn negation_and_not() {
        assert!(matches!(neg(int(5)).unwrap(), Value::Int(-5)));
        assert!(matches!(neg(float(2.5)).unwrap(), Value::Float(f) if f == -2.5));
        assert_eq!(neg(int(i64::MIN)).unwrap_err().kind, ErrorKind::Overflow);
        assert!(matches!(not_(int(0)).unwrap(), Value::Bool(true)));
        assert!(matches!(
            not_(Value::str("dog")).unwrap(),
            Value::Bool(false)
        ));
    }
}
