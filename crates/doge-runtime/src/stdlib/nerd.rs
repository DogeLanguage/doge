//! The `nerd` module: numeric helpers over Int and Float. Consts `pi`/`e` never
//! reach here — codegen emits them as `Value::Float` literals directly.

use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// A numeric argument as `f64`, or a catchable type error naming the function.
fn numeric(fname: &str, v: &Value) -> DogeResult<f64> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        _ => Err(DogeError::type_error(format!(
            "nerd.{fname} needs a number, got {}",
            v.describe()
        ))),
    }
}

/// Turn a `f64` result back into an Int, or a catchable Overflow when it will not
/// fit — so a floor/ceil/round that lands outside the Int range fails cleanly.
fn float_to_int(f: f64) -> DogeResult {
    if f.is_finite() && f >= i64::MIN as f64 && f < i64::MAX as f64 {
        Ok(Value::Int(f as i64))
    } else {
        Err(DogeError::overflow("the result is outside the Int range"))
    }
}

/// Shared by floor/ceil/round: an Int passes straight through, a Float is
/// transformed and narrowed back to an Int.
fn round_like(fname: &str, x: &Value, op: impl Fn(f64) -> f64) -> DogeResult {
    match x {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(f) => float_to_int(op(*f)),
        _ => Err(DogeError::type_error(format!(
            "nerd.{fname} needs a number, got {}",
            x.describe()
        ))),
    }
}

/// `nerd.abs(x)` — magnitude. An Int stays an Int (and `abs(i64::MIN)` overflows
/// catchably); a Float stays a Float.
pub fn nerd_abs(x: &Value) -> DogeResult {
    match x {
        Value::Int(n) => {
            if *n < 0 {
                n.checked_neg()
                    .map(Value::Int)
                    .ok_or_else(|| DogeError::overflow("the result is outside the Int range"))
            } else {
                Ok(Value::Int(*n))
            }
        }
        Value::Float(f) => Ok(Value::Float(f.abs())),
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
    round_like("floor", x, f64::floor)
}

/// `nerd.ceil(x)` — round toward positive infinity, yielding an Int.
pub fn nerd_ceil(x: &Value) -> DogeResult {
    round_like("ceil", x, f64::ceil)
}

/// `nerd.round(x)` — round half away from zero, yielding an Int.
pub fn nerd_round(x: &Value) -> DogeResult {
    round_like("round", x, f64::round)
}

/// `nerd.min(a, b)` — the smaller of two numbers, keeping the winner's own type;
/// a tie returns `a`.
pub fn nerd_min(a: &Value, b: &Value) -> DogeResult {
    let (x, y) = (numeric("min", a)?, numeric("min", b)?);
    Ok(if y < x { b.clone() } else { a.clone() })
}

/// `nerd.max(a, b)` — the larger of two numbers, keeping the winner's own type;
/// a tie returns `a`.
pub fn nerd_max(a: &Value, b: &Value) -> DogeResult {
    let (x, y) = (numeric("max", a)?, numeric("max", b)?);
    Ok(if y > x { b.clone() } else { a.clone() })
}

/// `nerd.pow(base, exponent)` — Int^Int (non-negative exponent) stays an Int and
/// overflows catchably; any other numeric mix returns a Float.
pub fn nerd_pow(a: &Value, b: &Value) -> DogeResult {
    match (a, b) {
        (Value::Int(base), Value::Int(exp)) if *exp >= 0 => {
            let exp = u32::try_from(*exp)
                .map_err(|_| DogeError::overflow("the result is outside the Int range"))?;
            base.checked_pow(exp)
                .map(Value::Int)
                .ok_or_else(|| DogeError::overflow("the result is outside the Int range"))
        }
        _ => {
            let (x, y) = (numeric("pow", a)?, numeric("pow", b)?);
            Ok(Value::Float(x.powf(y)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn abs_keeps_type_and_overflows_catchably() {
        assert!(matches!(nerd_abs(&Value::Int(-5)).unwrap(), Value::Int(5)));
        assert!(matches!(nerd_abs(&Value::Float(-2.5)).unwrap(), Value::Float(f) if f == 2.5));
        assert_eq!(
            nerd_abs(&Value::Int(i64::MIN)).unwrap_err().kind,
            ErrorKind::Overflow
        );
        assert_eq!(
            nerd_abs(&Value::str("x")).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn sqrt_is_float_and_rejects_negatives() {
        assert!(matches!(nerd_sqrt(&Value::Int(16)).unwrap(), Value::Float(f) if f == 4.0));
        assert_eq!(
            nerd_sqrt(&Value::Int(-1)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn floor_ceil_round_yield_ints() {
        assert!(matches!(
            nerd_floor(&Value::Float(2.9)).unwrap(),
            Value::Int(2)
        ));
        assert!(matches!(
            nerd_ceil(&Value::Float(2.1)).unwrap(),
            Value::Int(3)
        ));
        assert!(matches!(
            nerd_round(&Value::Float(2.5)).unwrap(),
            Value::Int(3)
        ));
        assert!(matches!(nerd_floor(&Value::Int(7)).unwrap(), Value::Int(7)));
    }

    #[test]
    fn round_out_of_range_is_overflow() {
        assert_eq!(
            nerd_floor(&Value::Float(1e300)).unwrap_err().kind,
            ErrorKind::Overflow
        );
    }

    #[test]
    fn min_max_keep_the_winners_type() {
        assert!(
            matches!(nerd_min(&Value::Int(3), &Value::Float(2.5)).unwrap(), Value::Float(f) if f == 2.5)
        );
        assert!(matches!(
            nerd_max(&Value::Int(3), &Value::Float(2.5)).unwrap(),
            Value::Int(3)
        ));
        // Ties return the first argument, unchanged.
        assert!(matches!(
            nerd_min(&Value::Int(4), &Value::Float(4.0)).unwrap(),
            Value::Int(4)
        ));
    }

    #[test]
    fn pow_is_int_for_int_base_and_overflows_catchably() {
        assert!(matches!(
            nerd_pow(&Value::Int(2), &Value::Int(10)).unwrap(),
            Value::Int(1024)
        ));
        assert!(matches!(
            nerd_pow(&Value::Int(2), &Value::Float(0.5)).unwrap(),
            Value::Float(_)
        ));
        assert_eq!(
            nerd_pow(&Value::Int(10), &Value::Int(100))
                .unwrap_err()
                .kind,
            ErrorKind::Overflow
        );
    }
}
