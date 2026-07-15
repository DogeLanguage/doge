use super::check_arity;
use crate::codec;
use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// Every method name [`str_method`] dispatches, for the bound-method gate
/// (`has_builtin_method`). Kept in step with the `match` below by a unit test.
pub(super) const STR_METHODS: &[&str] = &["from_b64", "from_hex"];

/// Dispatch a method call on a Str value. `from_b64`/`from_hex` decode the text
/// back into Bytes — the inverse of the Bytes `b64()`/`hex()` renderers — and raise
/// a catchable `ValueError` when the text is not valid for that encoding.
pub(super) fn str_method(recv: &Value, name: &str, args: Vec<Value>) -> DogeResult {
    let Value::Str(text) = recv else {
        unreachable!("compiler bug: str_method called on a non-Str")
    };
    let argc = args.len();
    match name {
        "from_b64" => {
            check_arity("Str", name, 0, argc)?;
            codec::b64_decode(text).map(Value::bytes).map_err(|_| {
                DogeError::value_error("cannot decode this Str as Bytes (not valid base64)")
            })
        }
        "from_hex" => {
            check_arity("Str", name, 0, argc)?;
            codec::hex_decode(text).map(Value::bytes).map_err(|_| {
                DogeError::value_error("cannot decode this Str as Bytes (not valid hex)")
            })
        }
        _ => Err(DogeError::attr_error(format!("a Str has no method {name}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn from_b64_round_trips_and_rejects() {
        let ok = str_method(&Value::str("aGk="), "from_b64", vec![]).unwrap();
        assert!(matches!(ok, Value::Bytes(b) if &*b == b"hi"));
        assert_eq!(
            str_method(&Value::str("aGk"), "from_b64", vec![])
                .unwrap_err()
                .kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn from_hex_round_trips_and_rejects() {
        let ok = str_method(&Value::str("6869"), "from_hex", vec![]).unwrap();
        assert!(matches!(ok, Value::Bytes(b) if &*b == b"hi"));
        assert_eq!(
            str_method(&Value::str("zz"), "from_hex", vec![])
                .unwrap_err()
                .kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn unknown_method_is_an_attr_error() {
        assert_eq!(
            str_method(&Value::str("x"), "nope", vec![])
                .unwrap_err()
                .kind,
            ErrorKind::AttrError
        );
    }
}
