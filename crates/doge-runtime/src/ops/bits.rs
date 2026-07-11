use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// The two Int operands of a bitwise binary operator, or a catchable type error
/// naming both values — bitwise operators are Int-only.
fn int_pair(sym: &str, a: &Value, b: &Value) -> DogeResult<(i64, i64)> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok((*x, *y)),
        _ => Err(DogeError::type_error(format!(
            "cannot {sym} {} and {} (bitwise operators need Ints)",
            a.describe(),
            b.describe()
        ))),
    }
}

/// A shift count as a `u32`: a negative count is a catchable value error, since
/// shifting by a negative amount has no meaning.
fn shift_count(y: i64) -> DogeResult<u32> {
    u32::try_from(y).map_err(|_| {
        DogeError::value_error(format!(
            "cannot shift by {y} — the shift count must be 0 or more"
        ))
    })
}

fn shift_overflow(x: i64, y: i64) -> DogeError {
    DogeError::overflow(format!("{x} << {y} overflowed the Int range"))
}

/// `&` — bitwise AND.
pub fn bitand(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair("&", &a, &b)?;
    Ok(Value::Int(x & y))
}

/// `|` — bitwise OR.
pub fn bitor(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair("|", &a, &b)?;
    Ok(Value::Int(x | y))
}

/// `^` — bitwise XOR.
pub fn bitxor(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair("^", &a, &b)?;
    Ok(Value::Int(x ^ y))
}

/// `<<` — left shift. Losing significant bits (or a count of 64 or more) is a
/// catchable overflow error, never a silent wraparound.
pub fn shl(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair("<<", &a, &b)?;
    let n = shift_count(y)?;
    if n >= i64::BITS {
        return Err(shift_overflow(x, y));
    }
    let result = x << n;
    // The shift is only lossless when it shifts back to the original value.
    if result >> n != x {
        return Err(shift_overflow(x, y));
    }
    Ok(Value::Int(result))
}

/// `>>` — arithmetic right shift. A count of 64 or more saturates to `0` for a
/// non-negative value and `-1` for a negative one, matching the sign fill.
pub fn shr(a: Value, b: Value) -> DogeResult {
    let (x, y) = int_pair(">>", &a, &b)?;
    let n = shift_count(y)?;
    let result = if n >= i64::BITS {
        if x < 0 {
            -1
        } else {
            0
        }
    } else {
        x >> n
    };
    Ok(Value::Int(result))
}

/// `~` — bitwise NOT (Int-only).
pub fn bitnot(a: Value) -> DogeResult {
    match &a {
        Value::Int(n) => Ok(Value::Int(!n)),
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

    fn int(n: i64) -> Value {
        Value::Int(n)
    }

    #[test]
    fn basic_bitwise() {
        assert!(matches!(
            bitand(int(0b1100), int(0b1010)).unwrap(),
            Value::Int(0b1000)
        ));
        assert!(matches!(
            bitor(int(0b1100), int(0b1010)).unwrap(),
            Value::Int(0b1110)
        ));
        assert!(matches!(
            bitxor(int(0b1100), int(0b1010)).unwrap(),
            Value::Int(0b0110)
        ));
        assert!(matches!(bitnot(int(0)).unwrap(), Value::Int(-1)));
        assert!(matches!(bitnot(int(5)).unwrap(), Value::Int(-6)));
    }

    #[test]
    fn shifts() {
        assert!(matches!(shl(int(1), int(4)).unwrap(), Value::Int(16)));
        assert!(matches!(shr(int(16), int(4)).unwrap(), Value::Int(1)));
        // Arithmetic right shift keeps the sign.
        assert!(matches!(shr(int(-8), int(1)).unwrap(), Value::Int(-4)));
        // A huge right-shift saturates to the sign fill.
        assert!(matches!(shr(int(-8), int(200)).unwrap(), Value::Int(-1)));
        assert!(matches!(shr(int(8), int(200)).unwrap(), Value::Int(0)));
    }

    #[test]
    fn shift_errors() {
        assert_eq!(shl(int(1), int(64)).unwrap_err().kind, ErrorKind::Overflow);
        assert_eq!(shl(int(1), int(63)).unwrap_err().kind, ErrorKind::Overflow);
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
