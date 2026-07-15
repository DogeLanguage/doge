use bigdecimal::{BigDecimal, Pow, RoundingMode, Signed, ToPrimitive, Zero};
use num_bigint::BigInt;

use super::{as_decimal, as_f64, is_decimal, is_float};
use crate::error::{DogeError, DogeResult};
use crate::value::Value;

fn type_err_binop(sym: &str, a: &Value, b: &Value) -> DogeError {
    DogeError::type_error(format!(
        "cannot {sym} {} and {}",
        a.describe(),
        b.describe()
    ))
}

fn div_by_zero(sym: &str) -> DogeError {
    DogeError::division_by_zero(format!("cannot {sym} by zero"))
}

/// A `Float`/`Decimal` arithmetic mix. Decimal is exact and Float is not, so
/// silently joining them would corrupt the exact value — the fix is to convert
/// one side explicitly.
fn mix_float_decimal(sym: &str, a: &Value, b: &Value) -> DogeError {
    DogeError::type_error(format!(
        "cannot {sym} {} and {} — Decimal is exact and Float is not; convert one with dec() or float()",
        a.describe(),
        b.describe()
    ))
}

/// Shared resolver for `+`/`-`/`*` once the type-specific arms (Int/Int, Str, …)
/// have been ruled out: exact `Decimal` math when a Decimal is involved (a
/// Float/Decimal mix is rejected), otherwise `Float` promotion.
fn numeric_binop(
    sym: &str,
    a: &Value,
    b: &Value,
    dec_op: impl Fn(BigDecimal, BigDecimal) -> BigDecimal,
    float_op: impl Fn(f64, f64) -> f64,
) -> DogeResult {
    if is_decimal(a) || is_decimal(b) {
        if is_float(a) || is_float(b) {
            return Err(mix_float_decimal(sym, a, b));
        }
        return match (as_decimal(a), as_decimal(b)) {
            (Some(x), Some(y)) => Ok(Value::decimal(dec_op(x, y))),
            _ => Err(type_err_binop(sym, a, b)),
        };
    }
    match (as_f64(a), as_f64(b)) {
        (Some(x), Some(y)) => Ok(Value::Float(float_op(x, y))),
        _ => Err(type_err_binop(sym, a, b)),
    }
}

/// Floor division on arbitrary-precision integers: truncate toward zero, then
/// nudge down one when the remainder is non-zero and the signs differ (Python).
fn bigint_floordiv(x: &BigInt, y: &BigInt) -> BigInt {
    let q = x / y;
    let r = x % y;
    if !r.is_zero() && (r.is_negative() != y.is_negative()) {
        q - BigInt::from(1)
    } else {
        q
    }
}

/// `floor(x / y)` as an exact integer-valued `Decimal`.
fn decimal_floordiv(x: &BigDecimal, y: &BigDecimal) -> BigDecimal {
    (x / y).with_scale_round(0, RoundingMode::Floor)
}

/// `+` — Int+Int (arbitrary precision), Float/Decimal promotion, Str/Bytes/List
/// concatenation, Error-as-message concatenation.
pub fn add(a: Value, b: Value) -> DogeResult {
    match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => Ok(Value::int(x + y)),
        (Value::Str(x), Value::Str(y)) => Ok(Value::str(format!("{x}{y}"))),
        (Value::Bytes(x), Value::Bytes(y)) => {
            let mut joined = Vec::with_capacity(x.len() + y.len());
            joined.extend_from_slice(x);
            joined.extend_from_slice(y);
            Ok(Value::bytes(joined))
        }
        // An Error concatenates with a Str as its message, so `"caught: " + err`
        // reads the same as barking the error. Every other `Str + x` stays a type
        // error — an Error is special only because its payload is text.
        (Value::Str(x), Value::Error(e)) => Ok(Value::str(format!("{x}{}", e.message))),
        (Value::Error(e), Value::Str(y)) => Ok(Value::str(format!("{}{y}", e.message))),
        (Value::List(x), Value::List(y)) => {
            let mut joined = x.borrow().clone();
            joined.extend(y.borrow().iter().cloned());
            Ok(Value::list(joined))
        }
        _ => numeric_binop("+", &a, &b, |x, y| x + y, |x, y| x + y),
    }
}

/// `-` — Int-Int (arbitrary precision) or Float/Decimal promotion.
pub fn sub(a: Value, b: Value) -> DogeResult {
    if let (Value::Int(x), Value::Int(y)) = (&a, &b) {
        return Ok(Value::int(x - y));
    }
    numeric_binop("-", &a, &b, |x, y| x - y, |x, y| x - y)
}

/// `*` — Int*Int (arbitrary precision) or Float/Decimal promotion.
pub fn mul(a: Value, b: Value) -> DogeResult {
    if let (Value::Int(x), Value::Int(y)) = (&a, &b) {
        return Ok(Value::int(x * y));
    }
    numeric_binop("*", &a, &b, |x, y| x * y, |x, y| x * y)
}

/// `/` — always a Float for integers (`5 / 2 == 2.5`); exact for decimals
/// (`Decimal / Decimal` → `Decimal`). A Float/Decimal mix is a type error.
pub fn div(a: Value, b: Value) -> DogeResult {
    if is_decimal(&a) || is_decimal(&b) {
        if is_float(&a) || is_float(&b) {
            return Err(mix_float_decimal("/", &a, &b));
        }
        return match (as_decimal(&a), as_decimal(&b)) {
            (Some(_), Some(y)) if y.is_zero() => Err(div_by_zero("/")),
            (Some(x), Some(y)) => Ok(Value::decimal(x / y)),
            _ => Err(type_err_binop("/", &a, &b)),
        };
    }
    match (as_f64(&a), as_f64(&b)) {
        (Some(_), Some(0.0)) => Err(div_by_zero("/")),
        (Some(x), Some(y)) => Ok(Value::Float(x / y)),
        _ => Err(type_err_binop("/", &a, &b)),
    }
}

/// `//` — floor division. Int//Int yields an Int, Decimal//Decimal an exact
/// integer-valued Decimal, any Float operand a floored Float.
pub fn floordiv(a: Value, b: Value) -> DogeResult {
    match (&a, &b) {
        (Value::Int(x), Value::Int(y)) => {
            if y.is_zero() {
                return Err(div_by_zero("//"));
            }
            Ok(Value::int(bigint_floordiv(x, y)))
        }
        _ if is_decimal(&a) || is_decimal(&b) => {
            if is_float(&a) || is_float(&b) {
                return Err(mix_float_decimal("//", &a, &b));
            }
            match (as_decimal(&a), as_decimal(&b)) {
                (Some(_), Some(y)) if y.is_zero() => Err(div_by_zero("//")),
                (Some(x), Some(y)) => Ok(Value::decimal(decimal_floordiv(&x, &y))),
                _ => Err(type_err_binop("//", &a, &b)),
            }
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
            if y.is_zero() {
                return Err(div_by_zero("%"));
            }
            let r = x % y;
            let m = if !r.is_zero() && (r.is_negative() != y.is_negative()) {
                r + y
            } else {
                r
            };
            Ok(Value::int(m))
        }
        _ if is_decimal(&a) || is_decimal(&b) => {
            if is_float(&a) || is_float(&b) {
                return Err(mix_float_decimal("%", &a, &b));
            }
            match (as_decimal(&a), as_decimal(&b)) {
                (Some(_), Some(y)) if y.is_zero() => Err(div_by_zero("%")),
                (Some(x), Some(y)) => {
                    let f = decimal_floordiv(&x, &y);
                    Ok(Value::decimal(&x - f * &y))
                }
                _ => Err(type_err_binop("%", &a, &b)),
            }
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

/// `**` — exponentiation. Int raised to a non-negative Int stays an Int
/// (arbitrary precision); a Decimal raised to a non-negative Int stays an exact
/// Decimal; a negative exponent or any Float operand promotes to Float. `0 **
/// <negative>` is a catchable division by zero, and an exponent too large to
/// materialize is a catchable overflow.
pub fn pow(a: Value, b: Value) -> DogeResult {
    match (&a, &b) {
        (Value::Int(base), Value::Int(exp)) if !exp.is_negative() => {
            let e = exp
                .to_u32()
                .ok_or_else(|| DogeError::overflow(format!("exponent {exp} is too large")))?;
            Ok(Value::int(Pow::pow(base.clone(), e)))
        }
        // A non-negative exponent is handled above, so the only Int base left with
        // an Int exponent here has a negative exponent: `0 ** <negative>` diverges.
        (Value::Int(base), Value::Int(_)) if base.is_zero() => Err(div_by_zero("**")),
        (Value::Decimal(base), Value::Int(exp)) if !exp.is_negative() => {
            let e = exp
                .to_u32()
                .ok_or_else(|| DogeError::overflow(format!("exponent {exp} is too large")))?;
            let mut result = BigDecimal::from(1);
            for _ in 0..e {
                result *= base;
            }
            Ok(Value::decimal(result))
        }
        _ if is_decimal(&a) || is_decimal(&b) => Err(DogeError::type_error(format!(
            "cannot raise {} to {} — a Decimal power needs a non-negative Int exponent",
            a.describe(),
            b.describe()
        ))),
        _ => match (as_f64(&a), as_f64(&b)) {
            (Some(x), Some(y)) if x == 0.0 && y < 0.0 => Err(div_by_zero("**")),
            (Some(x), Some(y)) => Ok(Value::Float(x.powf(y))),
            _ => Err(type_err_binop("**", &a, &b)),
        },
    }
}

/// Unary `-`.
pub fn neg(a: Value) -> DogeResult {
    match &a {
        Value::Int(n) => Ok(Value::int(-n)),
        Value::Float(f) => Ok(Value::Float(-f)),
        Value::Decimal(d) => Ok(Value::decimal(-d)),
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
