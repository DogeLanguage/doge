//! The `nerd` module: numeric helpers over Int, Float, and Decimal. Consts
//! `pi`/`e` never reach here — codegen emits them as `Value::Float` literals.

use std::cmp::Ordering;

use bigdecimal::{FromPrimitive, RoundingMode, Signed, ToPrimitive};
use num_bigint::BigInt;

use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// A numeric argument as `f64`, or a catchable type error naming the function.
/// Used by the always-Float helpers (`sqrt`); exactness is lost on purpose.
fn numeric(fname: &str, v: &Value) -> DogeResult<f64> {
    let f = match v {
        Value::Int(n) => n.to_f64(),
        Value::Float(f) => Some(*f),
        Value::Decimal(d) => d.to_f64(),
        _ => {
            return Err(DogeError::type_error(format!(
                "nerd.{fname} needs a number, got {}",
                v.describe()
            )))
        }
    };
    f.ok_or_else(|| DogeError::overflow(format!("{v} is too large for nerd.{fname}")))
}

/// Reject a non-numeric argument for the type-preserving helpers (`min`/`max`).
fn ensure_number(fname: &str, v: &Value) -> DogeResult<()> {
    if matches!(v, Value::Int(_) | Value::Float(_) | Value::Decimal(_)) {
        Ok(())
    } else {
        Err(DogeError::type_error(format!(
            "nerd.{fname} needs a number, got {}",
            v.describe()
        )))
    }
}

/// Turn a `f64` result back into an Int. Any finite value converts (Int is
/// arbitrary precision); only an infinity or NaN — which a floor/ceil/round can
/// never legitimately produce from a finite input — is a catchable Overflow.
fn float_to_int(f: f64) -> DogeResult {
    if f.is_finite() {
        BigInt::from_f64(f)
            .map(Value::Int)
            .ok_or_else(|| DogeError::overflow("the result is outside the Int range"))
    } else {
        Err(DogeError::overflow("the result is outside the Int range"))
    }
}

/// Shared by floor/ceil/round: an Int passes straight through, a Float is
/// transformed and narrowed back to an Int, a Decimal is rounded exactly to an
/// integer using `mode`.
fn round_like(
    fname: &str,
    x: &Value,
    float_op: impl Fn(f64) -> f64,
    mode: RoundingMode,
) -> DogeResult {
    match x {
        Value::Int(n) => Ok(Value::Int(n.clone())),
        Value::Float(f) => float_to_int(float_op(*f)),
        Value::Decimal(d) => {
            let (digits, _) = d.with_scale_round(0, mode).into_bigint_and_exponent();
            Ok(Value::Int(digits))
        }
        _ => Err(DogeError::type_error(format!(
            "nerd.{fname} needs a number, got {}",
            x.describe()
        ))),
    }
}

/// `nerd.abs(x)` — magnitude, keeping the argument's own type. An Int stays an
/// Int (and never overflows — it is arbitrary precision), a Float a Float, a
/// Decimal a Decimal.
pub fn nerd_abs(x: &Value) -> DogeResult {
    match x {
        Value::Int(n) => Ok(Value::Int(n.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        Value::Decimal(d) => Ok(Value::decimal(d.abs())),
        _ => Err(DogeError::type_error(format!(
            "nerd.abs needs a number, got {}",
            x.describe()
        ))),
    }
}

/// `nerd.sqrt(x)` — always a Float. A negative input is a catchable ValueError.
pub fn nerd_sqrt(x: &Value) -> DogeResult {
    let f = numeric("sqrt", x)?;
    if f < 0.0 {
        return Err(DogeError::value_error("cannot sqrt a negative number"));
    }
    Ok(Value::Float(f.sqrt()))
}

/// `nerd.floor(x)` — round toward negative infinity, yielding an Int.
pub fn nerd_floor(x: &Value) -> DogeResult {
    round_like("floor", x, f64::floor, RoundingMode::Floor)
}

/// `nerd.ceil(x)` — round toward positive infinity, yielding an Int.
pub fn nerd_ceil(x: &Value) -> DogeResult {
    round_like("ceil", x, f64::ceil, RoundingMode::Ceiling)
}

/// `nerd.round(x)` — round half away from zero, yielding an Int.
pub fn nerd_round(x: &Value) -> DogeResult {
    round_like("round", x, f64::round, RoundingMode::HalfUp)
}

/// `nerd.min(a, b)` — the smaller of two numbers, keeping the winner's own type;
/// a tie returns `a`. Compares exactly across Int/Decimal, via `f64` once a Float
/// is involved.
pub fn nerd_min(a: &Value, b: &Value) -> DogeResult {
    ensure_number("min", a)?;
    ensure_number("min", b)?;
    Ok(if crate::ops::order(a, b)? == Ordering::Greater {
        b.clone()
    } else {
        a.clone()
    })
}

/// `nerd.max(a, b)` — the larger of two numbers, keeping the winner's own type;
/// a tie returns `a`.
pub fn nerd_max(a: &Value, b: &Value) -> DogeResult {
    ensure_number("max", a)?;
    ensure_number("max", b)?;
    Ok(if crate::ops::order(a, b)? == Ordering::Less {
        b.clone()
    } else {
        a.clone()
    })
}

/// `nerd.pow(base, exponent)` — the same rule as the `**` operator: Int^Int (a
/// non-negative exponent) stays an arbitrary-precision Int, Decimal^Int an exact
/// Decimal, and any other numeric mix returns a Float.
pub fn nerd_pow(a: &Value, b: &Value) -> DogeResult {
    crate::ops::pow(a.clone(), b.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::ops::values_equal;

    fn dec(s: &str) -> Value {
        Value::decimal(s.parse().unwrap())
    }

    #[test]
    fn abs_keeps_type_and_never_overflows() {
        assert!(values_equal(
            &nerd_abs(&Value::int(-5)).unwrap(),
            &Value::int(5)
        ));
        assert!(matches!(nerd_abs(&Value::Float(-2.5)).unwrap(), Value::Float(f) if f == 2.5));
        assert!(values_equal(&nerd_abs(&dec("-2.5")).unwrap(), &dec("2.5")));
        // abs(i64::MIN) no longer overflows — it grows past the range.
        let expected = Value::Int(-BigInt::from(i64::MIN));
        assert!(values_equal(
            &nerd_abs(&Value::int(i64::MIN)).unwrap(),
            &expected
        ));
        assert_eq!(
            nerd_abs(&Value::str("x")).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn sqrt_is_float_and_rejects_negatives() {
        assert!(matches!(nerd_sqrt(&Value::int(16)).unwrap(), Value::Float(f) if f == 4.0));
        assert_eq!(
            nerd_sqrt(&Value::int(-1)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn floor_ceil_round_yield_ints() {
        assert!(values_equal(
            &nerd_floor(&Value::Float(2.9)).unwrap(),
            &Value::int(2)
        ));
        assert!(values_equal(
            &nerd_ceil(&Value::Float(2.1)).unwrap(),
            &Value::int(3)
        ));
        assert!(values_equal(
            &nerd_round(&Value::Float(2.5)).unwrap(),
            &Value::int(3)
        ));
        assert!(values_equal(
            &nerd_floor(&Value::int(7)).unwrap(),
            &Value::int(7)
        ));
        // Decimals round exactly.
        assert!(values_equal(
            &nerd_floor(&dec("2.9")).unwrap(),
            &Value::int(2)
        ));
        assert!(values_equal(
            &nerd_ceil(&dec("-2.9")).unwrap(),
            &Value::int(-2)
        ));
        assert!(values_equal(
            &nerd_round(&dec("2.5")).unwrap(),
            &Value::int(3)
        ));
    }

    #[test]
    fn round_of_a_non_finite_float_is_overflow() {
        assert_eq!(
            nerd_floor(&Value::Float(f64::INFINITY)).unwrap_err().kind,
            ErrorKind::Overflow
        );
    }

    #[test]
    fn min_max_keep_the_winners_type() {
        assert!(
            matches!(nerd_min(&Value::int(3), &Value::Float(2.5)).unwrap(), Value::Float(f) if f == 2.5)
        );
        assert!(values_equal(
            &nerd_max(&Value::int(3), &Value::Float(2.5)).unwrap(),
            &Value::int(3)
        ));
        // A Decimal can win and keeps its type.
        assert!(values_equal(
            &nerd_min(&Value::int(3), &dec("1.5")).unwrap(),
            &dec("1.5")
        ));
        // Ties return the first argument, unchanged.
        assert!(values_equal(
            &nerd_min(&Value::int(4), &Value::Float(4.0)).unwrap(),
            &Value::int(4)
        ));
        assert_eq!(
            nerd_min(&Value::str("a"), &Value::str("b"))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn pow_is_int_for_int_base_and_grows_past_i64() {
        assert!(values_equal(
            &nerd_pow(&Value::int(2), &Value::int(10)).unwrap(),
            &Value::int(1024)
        ));
        assert!(matches!(
            nerd_pow(&Value::int(2), &Value::Float(0.5)).unwrap(),
            Value::Float(_)
        ));
        // 10^100 is exact now, never an overflow.
        let expected = Value::Int(BigInt::from(10).pow(100u32));
        assert!(values_equal(
            &nerd_pow(&Value::int(10), &Value::int(100)).unwrap(),
            &expected
        ));
    }
}
