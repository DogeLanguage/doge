use super::{check_arity, expect_bytes, expect_int};
use crate::codec;
use crate::error::{DogeError, DogeResult};
use crate::objects::method_arity_error;
use crate::value::Value;

/// Every method name [`bytes_method`] dispatches, for the bound-method gate
/// (`has_builtin_method`). Kept in step with the `match` below by a unit test.
pub(super) const BYTES_METHODS: &[&str] = &["hex", "b64", "decode", "find", "split", "contains"];

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
        // `find(needle)` / `find(needle, start)`: the byte offset of the first
        // `needle` at or after `start`, or -1 when absent. An empty needle is
        // always present, matching `bytes in bytes`.
        "find" => {
            if argc != 1 && argc != 2 {
                return Err(method_arity_error("Bytes", name, 1, Some(2), argc));
            }
            let mut args = args.into_iter();
            let needle = expect_bytes(
                args.next().expect("checked arity"),
                "Bytes.find needs a Bytes needle",
            )?;
            let start = match args.next() {
                Some(v) => expect_int(v, "Bytes.find needs an Int start")?.max(0) as usize,
                None => 0,
            };
            let start = start.min(bytes.len());
            if needle.is_empty() {
                return Ok(Value::int(start as i64));
            }
            let offset = bytes[start..]
                .windows(needle.len())
                .position(|w| w == &needle[..])
                .map(|p| (p + start) as i64)
                .unwrap_or(-1);
            Ok(Value::int(offset))
        }
        // `split(sep)`: the pieces of the Bytes between each non-overlapping
        // `sep`, empty pieces kept (mirrors `strings.split`). An empty separator
        // is a catchable ValueError.
        "split" => {
            check_arity("Bytes", name, 1, argc)?;
            let sep = expect_bytes(
                args.into_iter().next().expect("checked arity"),
                "Bytes.split needs a Bytes separator",
            )?;
            if sep.is_empty() {
                return Err(DogeError::value_error("cannot split on an empty Bytes"));
            }
            let mut pieces = Vec::new();
            let mut start = 0;
            let mut i = 0;
            while i + sep.len() <= bytes.len() {
                if bytes[i..i + sep.len()] == sep[..] {
                    pieces.push(Value::bytes(&bytes[start..i]));
                    i += sep.len();
                    start = i;
                } else {
                    i += 1;
                }
            }
            pieces.push(Value::bytes(&bytes[start..]));
            Ok(Value::list(pieces))
        }
        // `contains(needle)`: whether `needle` occurs as a contiguous sub-slice.
        // An empty needle is always present, matching `bytes in bytes`.
        "contains" => {
            check_arity("Bytes", name, 1, argc)?;
            let needle = expect_bytes(
                args.into_iter().next().expect("checked arity"),
                "Bytes.contains needs a Bytes needle",
            )?;
            let found = needle.is_empty()
                || (needle.len() <= bytes.len()
                    && bytes.windows(needle.len()).any(|w| w == &needle[..]));
            Ok(Value::Bool(found))
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

    fn as_int(v: Value) -> i64 {
        use bigdecimal::ToPrimitive;
        match v {
            Value::Int(n) => n.to_i64().expect("small int"),
            other => panic!("expected an Int, got {other:?}"),
        }
    }

    fn as_bytes(v: &Value) -> Vec<u8> {
        match v {
            Value::Bytes(b) => b.to_vec(),
            other => panic!("expected Bytes, got {other:?}"),
        }
    }

    #[test]
    fn find_returns_offset_or_minus_one() {
        let body = Value::bytes("--X--preamble--X--payload".as_bytes());
        let needle = Value::bytes("--X--".as_bytes());
        assert_eq!(
            as_int(bytes_method(&body, "find", vec![needle.clone()]).unwrap()),
            0
        );
        // The second "--X--" starts at byte 13; searching from 1 skips the first.
        assert_eq!(
            as_int(bytes_method(&body, "find", vec![needle.clone(), Value::int(1)]).unwrap()),
            13
        );
        // A start past the last match, and an absent needle, both yield -1.
        assert_eq!(
            as_int(bytes_method(&body, "find", vec![needle, Value::int(14)]).unwrap()),
            -1
        );
        assert_eq!(
            as_int(bytes_method(&body, "find", vec![Value::bytes("zzz".as_bytes())]).unwrap()),
            -1
        );
    }

    #[test]
    fn find_empty_needle_returns_clamped_start() {
        let b = Value::bytes([1, 2, 3]);
        assert_eq!(
            as_int(bytes_method(&b, "find", vec![Value::bytes([])]).unwrap()),
            0
        );
        assert_eq!(
            as_int(bytes_method(&b, "find", vec![Value::bytes([]), Value::int(99)]).unwrap()),
            3
        );
    }

    #[test]
    fn find_needs_a_bytes_and_valid_arity() {
        let b = Value::bytes([1, 2, 3]);
        assert_eq!(
            bytes_method(&b, "find", vec![Value::int(1)])
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            bytes_method(&b, "find", vec![]).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn split_keeps_empty_pieces_and_rejects_empty_sep() {
        let body = Value::bytes("--X--preamble--X--payload".as_bytes());
        let sep = Value::bytes("--X--".as_bytes());
        match bytes_method(&body, "split", vec![sep]).unwrap() {
            Value::List(items) => {
                let items = items.borrow();
                assert_eq!(items.len(), 3);
                assert!(as_bytes(&items[0]).is_empty());
                assert_eq!(as_bytes(&items[1]), b"preamble");
                assert_eq!(as_bytes(&items[2]), b"payload");
            }
            other => panic!("expected a list, got {other:?}"),
        }
        assert_eq!(
            bytes_method(&body, "split", vec![Value::bytes([])])
                .unwrap_err()
                .kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn contains_reports_sub_slice_presence() {
        let body = Value::bytes("--X--payload".as_bytes());
        assert!(matches!(
            bytes_method(&body, "contains", vec![Value::bytes("payload".as_bytes())]).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            bytes_method(&body, "contains", vec![Value::bytes("zzz".as_bytes())]).unwrap(),
            Value::Bool(false)
        ));
        // An empty needle is always present, matching `bytes in bytes`.
        assert!(matches!(
            bytes_method(&body, "contains", vec![Value::bytes([])]).unwrap(),
            Value::Bool(true)
        ));
    }
}
