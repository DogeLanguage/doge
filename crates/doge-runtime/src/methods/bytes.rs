use super::check_arity;
use crate::codec;
use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// Every method name [`bytes_method`] dispatches, for the bound-method gate
/// (`has_builtin_method`). Kept in step with the `match` below by a unit test.
pub(super) const BYTES_METHODS: &[&str] = &["hex", "b64", "decode"];

/// Dispatch a method call on a Bytes value. `hex`/`b64` render the bytes as a
/// lowercase hex or standard-base64 Str; `decode` turns them back into text, a
/// catchable `ValueError` when they are not valid UTF-8.
pub(super) fn bytes_method(recv: &Value, name: &str, args: Vec<Value>) -> DogeResult {
    let Value::Bytes(bytes) = recv else {
        unreachable!("compiler bug: bytes_method called on a non-Bytes")
    };
    let argc = args.len();
    match name {
        "hex" => {
            check_arity("Bytes", name, 0, argc)?;
            let mut out = String::with_capacity(bytes.len() * 2);
            for byte in bytes.iter() {
                out.push_str(&format!("{byte:02x}"));
            }
            Ok(Value::str(out))
        }
        "b64" => {
            check_arity("Bytes", name, 0, argc)?;
            Ok(Value::str(codec::b64_encode(bytes)))
        }
        "decode" => {
            check_arity("Bytes", name, 0, argc)?;
            match std::str::from_utf8(bytes) {
                Ok(text) => Ok(Value::str(text)),
                Err(_) => Err(DogeError::value_error(
                    "cannot decode these Bytes as text (not valid UTF-8)",
                )),
            }
        }
        _ => Err(DogeError::attr_error(format!(
            "a Bytes has no method {name}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn hex_renders_lowercase() {
        let b = Value::bytes([0x00, 0xff, 0x68, 0x69]);
        assert!(
            matches!(bytes_method(&b, "hex", vec![]).unwrap(), Value::Str(s) if &*s == "00ff6869")
        );
    }

    #[test]
    fn b64_renders_standard_padded() {
        let b = Value::bytes("hi".as_bytes());
        assert!(matches!(bytes_method(&b, "b64", vec![]).unwrap(), Value::Str(s) if &*s == "aGk="));
        let empty = Value::bytes([]);
        assert!(
            matches!(bytes_method(&empty, "b64", vec![]).unwrap(), Value::Str(s) if s.is_empty())
        );
    }

    #[test]
    fn decode_round_trips_utf8_and_rejects_invalid() {
        let b = Value::bytes("héllo".as_bytes());
        assert!(
            matches!(bytes_method(&b, "decode", vec![]).unwrap(), Value::Str(s) if &*s == "héllo")
        );
        let bad = Value::bytes([0xff, 0xfe]);
        assert_eq!(
            bytes_method(&bad, "decode", vec![]).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn unknown_method_is_an_attr_error() {
        let b = Value::bytes([1, 2, 3]);
        assert_eq!(
            bytes_method(&b, "nope", vec![]).unwrap_err().kind,
            ErrorKind::AttrError
        );
    }
}
