use super::as_f64;
use crate::error::{DogeError, DogeResult};
use crate::value::Value;

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

/// `/` — always returns a Float (`5 / 2 == 2.5`), per the sharp-edges table in docs/README.md.
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
