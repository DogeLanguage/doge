use crate::error::DogeResult;
use crate::value::Value;

/// `bark x` — print a value on its own line and evaluate to `none`, so
/// `bark` can sit anywhere an expression is expected.
pub fn bark(v: &Value) -> Value {
    println!("{v}");
    Value::None
}

/// `len(x)` — character count for a Str, element count for a List or Dict.
/// Anything else is a catchable type error.
pub fn len(v: &Value) -> DogeResult {
    match v {
        Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
        Value::List(items) => Ok(Value::Int(items.borrow().len() as i64)),
        Value::Dict(entries) => Ok(Value::Int(entries.borrow().len() as i64)),
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

/// `int(x)` — Int unchanged, Float truncated toward zero, Bool to 0/1, a Str
/// parsed as a whole number. A Str that isn't a number is a catchable
/// `ValueError`; other types are a `TypeError`.
pub fn to_int(v: &Value) -> DogeResult {
    match v {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(f) => Ok(Value::Int(*f as i64)),
        Value::Bool(b) => Ok(Value::Int(i64::from(*b))),
        Value::Str(s) => s.trim().parse::<i64>().map(Value::Int).map_err(|_| {
            crate::error::DogeError::value_error(format!("cannot turn {s:?} into an Int"))
        }),
        _ => Err(crate::error::DogeError::type_error(format!(
            "cannot turn {} into an Int",
            v.describe()
        ))),
    }
}

/// `float(x)` — Int and Bool widen to Float, Float is unchanged, a numeric Str
/// is parsed. A non-numeric Str is a catchable `ValueError`; other types are a
/// `TypeError`.
pub fn to_float(v: &Value) -> DogeResult {
    match v {
        Value::Int(n) => Ok(Value::Float(*n as f64)),
        Value::Float(f) => Ok(Value::Float(*f)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn bark_returns_none() {
        assert!(matches!(bark(&Value::str("much hello")), Value::None));
    }

    #[test]
    fn len_counts_characters_and_elements() {
        // Char count, not byte count — 'é' is one character.
        assert!(matches!(len(&Value::str("héllo")).unwrap(), Value::Int(5)));
        assert!(matches!(
            len(&Value::list(vec![Value::Int(1), Value::Int(2)])).unwrap(),
            Value::Int(2)
        ));
        assert_eq!(len(&Value::Int(3)).unwrap_err().kind, ErrorKind::TypeError);
    }

    #[test]
    fn conversions_round_trip() {
        assert!(matches!(to_str(&Value::Int(7)), Value::Str(s) if &*s == "7"));
        assert!(matches!(to_int(&Value::Float(3.9)).unwrap(), Value::Int(3)));
        assert!(matches!(
            to_int(&Value::str(" 42 ")).unwrap(),
            Value::Int(42)
        ));
        assert!(matches!(to_float(&Value::Int(4)).unwrap(), Value::Float(f) if f == 4.0));
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
}
