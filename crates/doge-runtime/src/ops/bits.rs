use bigdecimal::Signed;
use num_bigint::BigInt;

use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// The two Int operands of a bitwise binary operator, or a catchable type error
/// naming both values — bitwise operators are Int-only.
fn int_pair<'a>(sym: &str, a: &'a Value, b: &'a Value) -> DogeResult<(&'a BigInt, &'a BigInt)> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok((x, y)),
        _ => Err(DogeError::type_error(format!(
            "cannot {sym} {} and {} (bitwise operators need Ints)",
            a.describe(),
            b.describe()
        ))),
    }
}

/// A shift count as a `usize`: a negative count is a catchable value error, and a
/// count too large to address is one too (a left shift by it could never fit in
/// memory). `Int` is arbitrary precision, so a *left* shift never drops bits —
/// only the count itself can be out of range.
fn shift_count(y: &BigInt) -> DogeResult<usize> {
    if y.is_negative() {
        return Err(DogeError::value_error(format!(
            "cannot shift by {y} — the shift count must be 0 or more"
        )));
    }
    usize::try_from(y).map_err(|_| {
        DogeError::value_error(format!(
            "cannot shift by {y} — the shift count is too large"
        ))
    })
}

/// `&` — bitwise AND.
pub fn bitand(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair("&", &a, &b)?;
    Ok(Value::int(x & y))
}

/// `|` — bitwise OR.
pub fn bitor(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair("|", &a, &b)?;
    Ok(Value::int(x | y))
}

/// `^` — bitwise XOR.
pub fn bitxor(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair("^", &a, &b)?;
    Ok(Value::int(x ^ y))
}

/// `<<` — left shift. `Int` is arbitrary precision, so this never drops
/// significant bits; only a negative or too-large shift count is a catchable error.
pub fn shl(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair("<<", &a, &b)?;
    let n = shift_count(y)?;
    Ok(Value::int(x << n))
}

/// `>>` — arithmetic (sign-preserving) right shift. A count larger than the value
/// has bits saturates to the sign fill: `0` for a non-negative value, `-1` for a
/// negative one.
pub fn shr(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair(">>", &a, &b)?;
    if y.is_negative() {
        return Err(DogeError::value_error(format!(
            "cannot shift by {y} — the shift count must be 0 or more"
        )));
    }
    // A count that doesn't fit a `usize` shifts every bit out; the result is the
    // sign fill without materializing an absurd shift.
    match usize::try_from(y) {
        Ok(n) => Ok(Value::int(x >> n)),
        Err(_) => Ok(Value::int(if x.is_negative() {
            BigInt::from(-1)
        } else {
            BigInt::from(0)
        })),
    }
}

/// `~` — bitwise NOT (Int-only).
pub fn bitnot(a: Value) -> DogeResult {
    match &a {
        Value::Int(n) => Ok(Value::int(!n)),
        _ => Err(DogeError::type_error(format!(
            "cannot ~ {} (bitwise NOT needs an Int)",
            a.describe()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::ops::compare::values_equal;

    fn int(n: i64) -> Value {
        Value::int(n)
    }

    #[test]
    fn basic_bitwise() {
        assert!(values_equal(
            &bitand(int(0b1100), int(0b1010)).unwrap(),
            &int(0b1000)
        ));
        assert!(values_equal(
            &bitor(int(0b1100), int(0b1010)).unwrap(),
            &int(0b1110)
        ));
        assert!(values_equal(
            &bitxor(int(0b1100), int(0b1010)).unwrap(),
            &int(0b0110)
        ));
        assert!(values_equal(&bitnot(int(0)).unwrap(), &int(-1)));
        assert!(values_equal(&bitnot(int(5)).unwrap(), &int(-6)));
    }

    #[test]
    fn shifts() {
        assert!(values_equal(&shl(int(1), int(4)).unwrap(), &int(16)));
        assert!(values_equal(&shr(int(16), int(4)).unwrap(), &int(1)));
        // Arithmetic right shift keeps the sign.
        assert!(values_equal(&shr(int(-8), int(1)).unwrap(), &int(-4)));
        // A huge right-shift saturates to the sign fill.
        assert!(values_equal(&shr(int(-8), int(200)).unwrap(), &int(-1)));
        assert!(values_equal(&shr(int(8), int(200)).unwrap(), &int(0)));
    }

    #[test]
    fn left_shift_grows_instead_of_overflowing() {
        // `1 << 64` used to overflow i64; with arbitrary precision it is 2^64.
        let two_pow_64: BigInt = BigInt::from(1) << 64u32;
        assert!(values_equal(
            &shl(int(1), int(64)).unwrap(),
            &Value::int(two_pow_64)
        ));
    }

    #[test]
    fn shift_errors() {
        assert_eq!(
            shl(int(-1), int(-1)).unwrap_err().kind,
            ErrorKind::ValueError
        );
        assert_eq!(
            shr(int(1), int(-1)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn non_int_is_a_type_error() {
        assert_eq!(
            bitand(int(1), Value::str("x")).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            bitnot(Value::str("x")).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
