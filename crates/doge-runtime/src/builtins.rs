use std::io::Write;

use bigdecimal::{BigDecimal, FromPrimitive, RoundingMode, ToPrimitive};
use num_bigint::BigInt;

use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// `bark x` — print a value on its own line and evaluate to `none`, so
/// `bark` can sit anywhere an expression is expected.
pub fn bark(v: &Value) -> Value {
    println!("{v}");
    Value::None
}

/// `gib()` / `gib("prompt")` — read one line from standard input. An optional
/// prompt, which must be a Str, is written without a trailing newline and flushed
/// first. The returned Str has its trailing newline stripped; at end of input the
/// result is `none`. A stdin read failure is a catchable IOError.
pub fn gib(prompt: Option<&Value>) -> DogeResult {
    if let Some(p) = prompt {
        let text = match p {
            Value::Str(s) => s,
            _ => {
                return Err(DogeError::type_error(format!(
                    "gib needs a Str prompt, got {}",
                    p.describe()
                )))
            }
        };
        print!("{text}");
        let _ = std::io::stdout().flush();
    }
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => Ok(Value::None),
        Ok(_) => {
            let line = line.strip_suffix('\n').unwrap_or(&line);
            let line = line.strip_suffix('\r').unwrap_or(line);
            Ok(Value::str(line))
        }
        Err(err) => Err(DogeError::io_error(format!("could not read input: {err}"))),
    }
}

/// `len(x)` — character count for a Str, byte count for a Bytes, element count
/// for a List or Dict. Anything else is a catchable type error.
pub fn len(v: &Value) -> DogeResult {
    match v {
        Value::Str(s) => Ok(Value::int(s.chars().count())),
        Value::Bytes(b) => Ok(Value::int(b.len())),
        Value::List(items) => Ok(Value::int(items.borrow().len())),
        Value::Dict(entries) => Ok(Value::int(entries.borrow().len())),
        _ => Err(crate::error::DogeError::type_error(format!(
            "cannot take the len of {}",
            v.describe()
        ))),
    }
}

/// `str(x)` — the value's printed form as a Str. Always succeeds.
pub fn to_str(v: &Value) -> Value {
    Value::str(v.to_string())
}

/// String interpolation (`"a {b} c"`) — join each part's display form, the same
/// text `bark`/`str` would show, into one Str. Always succeeds.
pub fn interp(parts: &[Value]) -> Value {
    let mut out = String::new();
    for part in parts {
        out.push_str(&part.to_string());
    }
    Value::str(out)
}

/// `int(x)` — Int unchanged, Float and Decimal truncated toward zero, Bool to
/// 0/1, a Str parsed as a whole number (of any size). A Str that isn't a number
/// is a catchable `ValueError`; other types are a `TypeError`.
pub fn to_int(v: &Value) -> DogeResult {
    match v {
        Value::Int(n) => Ok(Value::Int(n.clone())),
        Value::Float(f) => BigInt::from_f64(f.trunc())
            .map(Value::Int)
            .ok_or_else(|| DogeError::value_error(format!("cannot turn {f} into an Int"))),
        Value::Decimal(d) => {
            let (digits, _) = d
                .with_scale_round(0, RoundingMode::Down)
                .into_bigint_and_exponent();
            Ok(Value::Int(digits))
        }
        Value::Bool(b) => Ok(Value::int(i64::from(*b))),
        Value::Str(s) => s.trim().parse::<BigInt>().map(Value::Int).map_err(|_| {
            crate::error::DogeError::value_error(format!("cannot turn {s:?} into an Int"))
        }),
        _ => Err(crate::error::DogeError::type_error(format!(
            "cannot turn {} into an Int",
            v.describe()
        ))),
    }
}

/// `float(x)` — Int and Bool widen to Float, Decimal rounds to the nearest Float,
/// Float is unchanged, a numeric Str is parsed. A non-numeric Str is a catchable
/// `ValueError`; other types are a `TypeError`.
pub fn to_float(v: &Value) -> DogeResult {
    match v {
        Value::Int(n) => n
            .to_f64()
            .map(Value::Float)
            .ok_or_else(|| DogeError::value_error(format!("{n} is too large for a Float"))),
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Decimal(d) => d
            .to_f64()
            .map(Value::Float)
            .ok_or_else(|| DogeError::value_error(format!("{d} is too large for a Float"))),
        Value::Bool(b) => Ok(Value::Float(if *b { 1.0 } else { 0.0 })),
        Value::Str(s) => s.trim().parse::<f64>().map(Value::Float).map_err(|_| {
            crate::error::DogeError::value_error(format!("cannot turn {s:?} into a Float"))
        }),
        _ => Err(crate::error::DogeError::type_error(format!(
            "cannot turn {} into a Float",
            v.describe()
        ))),
    }
}

/// `dec(x)` — an exact `Decimal`. An Int converts exactly; a Bool to 0/1; a
/// numeric Str parses exactly (`dec("0.10")`); a Float converts via its shortest
/// decimal form, so `dec(0.1)` is exactly `0.1` and not the binary noise a raw
/// cast would carry. A Decimal is returned unchanged. A non-numeric Str is a
/// catchable `ValueError`; other types are a `TypeError`.
pub fn to_decimal(v: &Value) -> DogeResult {
    match v {
        Value::Decimal(_) => Ok(v.clone()),
        Value::Int(n) => Ok(Value::decimal(BigDecimal::from(n.clone()))),
        Value::Bool(b) => Ok(Value::decimal(BigDecimal::from(i64::from(*b)))),
        Value::Float(f) => format!("{f}")
            .parse::<BigDecimal>()
            .map(Value::decimal)
            .map_err(|_| DogeError::value_error(format!("cannot turn {f} into a Decimal"))),
        Value::Str(s) => s
            .trim()
            .parse::<BigDecimal>()
            .map(Value::decimal)
            .map_err(|_| DogeError::value_error(format!("cannot turn {s:?} into a Decimal"))),
        _ => Err(DogeError::type_error(format!(
            "cannot turn {} into a Decimal",
            v.describe()
        ))),
    }
}

/// `bytes(x)` — raw bytes from a value. A Str is UTF-8 encoded; a List of Ints
/// becomes those bytes, each of which must be in `0..=255` (a catchable
/// `ValueError` otherwise); a Bytes is returned unchanged. Any other type is a
/// catchable `TypeError`.
pub fn to_bytes(v: &Value) -> DogeResult {
    match v {
        Value::Bytes(_) => Ok(v.clone()),
        Value::Str(s) => Ok(Value::bytes(s.as_bytes())),
        Value::List(items) => {
            let items = items.borrow();
            let mut out = Vec::with_capacity(items.len());
            for item in items.iter() {
                match item {
                    Value::Int(n) => {
                        let byte = n.to_u8().ok_or_else(|| {
                            crate::error::DogeError::value_error(format!(
                                "bytes needs each Int in 0..=255, got {n}"
                            ))
                        })?;
                        out.push(byte);
                    }
                    other => {
                        return Err(crate::error::DogeError::type_error(format!(
                            "bytes needs a List of Ints, got {} in the List",
                            other.describe()
                        )))
                    }
                }
            }
            Ok(Value::bytes(out))
        }
        _ => Err(crate::error::DogeError::type_error(format!(
            "cannot turn {} into Bytes",
            v.describe()
        ))),
    }
}

/// `range(start, end)` — the Ints `start, start+1, …, end-1` as an eager List.
/// When `end <= start` the List is naturally empty. Both arguments must be Int;
/// anything else is a catchable type error. A bound too large to be a machine
/// index is a catchable value error (the List could never fit in memory anyway).
/// The one-argument Doge form `range(n)` is compiled as `range(0, n)`, so the
/// runtime has one signature.
pub fn range(start: &Value, end: &Value) -> DogeResult {
    match (start, end) {
        (Value::Int(a), Value::Int(b)) => {
            let a = a.to_i64().ok_or_else(|| range_bound_too_large(a))?;
            let b = b.to_i64().ok_or_else(|| range_bound_too_large(b))?;
            Ok(Value::list((a..b).map(Value::int).collect()))
        }
        (Value::Int(_), other) | (other, _) => Err(crate::error::DogeError::type_error(format!(
            "range needs Int bounds, got {}",
            other.describe()
        ))),
    }
}

fn range_bound_too_large(n: &BigInt) -> DogeError {
    DogeError::value_error(format!("range bound {n} is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::ops::values_equal;

    #[test]
    fn bark_returns_none() {
        assert!(matches!(bark(&Value::str("much hello")), Value::None));
    }

    #[test]
    fn len_counts_characters_and_elements() {
        // Char count, not byte count — 'é' is one character.
        assert!(values_equal(
            &len(&Value::str("héllo")).unwrap(),
            &Value::int(5)
        ));
        assert!(values_equal(
            &len(&Value::list(vec![Value::int(1), Value::int(2)])).unwrap(),
            &Value::int(2)
        ));
        assert_eq!(len(&Value::int(3)).unwrap_err().kind, ErrorKind::TypeError);
    }

    #[test]
    fn interp_joins_display_forms() {
        let parts = [
            Value::str("age "),
            Value::int(7),
            Value::str(", "),
            Value::None,
        ];
        assert!(matches!(interp(&parts), Value::Str(s) if &*s == "age 7, none"));
        // Nested Strs embed bare, matching bark/str, not the quoted repr.
        let nested = [Value::str("["), Value::str("hi"), Value::str("]")];
        assert!(matches!(interp(&nested), Value::Str(s) if &*s == "[hi]"));
        assert!(matches!(interp(&[]), Value::Str(s) if s.is_empty()));
    }

    #[test]
    fn conversions_round_trip() {
        assert!(matches!(to_str(&Value::int(7)), Value::Str(s) if &*s == "7"));
        assert!(values_equal(
            &to_int(&Value::Float(3.9)).unwrap(),
            &Value::int(3)
        ));
        assert!(values_equal(
            &to_int(&Value::str(" 42 ")).unwrap(),
            &Value::int(42)
        ));
        assert!(matches!(to_float(&Value::int(4)).unwrap(), Value::Float(f) if f == 4.0));
    }

    #[test]
    fn int_parses_arbitrary_precision_strings() {
        // A value far past i64 parses exactly as an Int.
        let big = "123456789012345678901234567890";
        let parsed = to_int(&Value::str(big)).unwrap();
        assert_eq!(parsed.to_string(), big);
    }

    #[test]
    fn dec_is_exact_from_str_int_and_float() {
        assert_eq!(to_decimal(&Value::str("0.10")).unwrap().to_string(), "0.10");
        assert!(values_equal(
            &to_decimal(&Value::int(3)).unwrap(),
            &to_decimal(&Value::str("3")).unwrap()
        ));
        // A Float converts via its shortest decimal form, not the binary noise.
        assert_eq!(to_decimal(&Value::Float(0.1)).unwrap().to_string(), "0.1");
        assert_eq!(
            to_decimal(&Value::str("nope")).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn int_truncates_a_decimal_toward_zero() {
        assert!(values_equal(
            &to_int(&to_decimal(&Value::str("3.9")).unwrap()).unwrap(),
            &Value::int(3)
        ));
        assert!(values_equal(
            &to_int(&to_decimal(&Value::str("-3.9")).unwrap()).unwrap(),
            &Value::int(-3)
        ));
    }

    #[test]
    fn bad_conversions_are_catchable_value_errors() {
        assert_eq!(
            to_int(&Value::str("dog")).unwrap_err().kind,
            ErrorKind::ValueError
        );
        assert_eq!(
            to_float(&Value::str("woof")).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn to_bytes_from_str_list_and_bytes() {
        // A Str UTF-8-encodes: 'é' is the two bytes 0xC3 0xA9.
        assert!(matches!(
            to_bytes(&Value::str("é")).unwrap(),
            Value::Bytes(b) if b[..] == [0xc3, 0xa9]
        ));
        // A List of Ints becomes those bytes.
        let from_list = to_bytes(&Value::list(vec![Value::int(104), Value::int(105)])).unwrap();
        assert!(matches!(from_list, Value::Bytes(b) if b[..] == [104, 105]));
        // A Bytes is returned unchanged.
        assert!(matches!(
            to_bytes(&Value::bytes([1, 2])).unwrap(),
            Value::Bytes(b) if b[..] == [1, 2]
        ));
    }

    #[test]
    fn to_bytes_rejects_out_of_range_and_wrong_types() {
        assert_eq!(
            to_bytes(&Value::list(vec![Value::int(256)]))
                .unwrap_err()
                .kind,
            ErrorKind::ValueError
        );
        assert_eq!(
            to_bytes(&Value::list(vec![Value::str("x")]))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            to_bytes(&Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn len_counts_bytes_for_bytes() {
        assert!(values_equal(
            &len(&Value::bytes("héllo")).unwrap(),
            &Value::int(6)
        ));
    }

    #[test]
    fn range_two_args() {
        let xs = range(&Value::int(2), &Value::int(5)).unwrap();
        match xs {
            Value::List(items) => {
                let items = items.borrow();
                assert_eq!(items.len(), 3);
                assert!(values_equal(&items[0], &Value::int(2)));
                assert!(values_equal(&items[2], &Value::int(4)));
            }
            _ => panic!("expected a list"),
        }
    }

    #[test]
    fn range_empty_when_end_not_after_start() {
        let xs = range(&Value::int(5), &Value::int(5)).unwrap();
        assert!(values_equal(&len(&xs).unwrap(), &Value::int(0)));
        let ys = range(&Value::int(5), &Value::int(2)).unwrap();
        assert!(values_equal(&len(&ys).unwrap(), &Value::int(0)));
    }

    #[test]
    fn range_rejects_float() {
        assert_eq!(
            range(&Value::int(0), &Value::Float(3.0)).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            range(&Value::Float(0.0), &Value::int(3)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
