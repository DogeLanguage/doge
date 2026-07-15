//! `crypto` — the security-primitives stdlib module. A deliberately tiny,
//! hard-to-misuse surface: `sha256` and `hmac_sha256` hash a `Str` or `Bytes` to a
//! 32-byte `Bytes` digest, `token` draws cryptographically secure random `Bytes`
//! from the OS CSPRNG (independent of the clock-seeded `roll` PRNG), and `same`
//! compares two `Str`/`Bytes` in constant time so secrets never leak via timing.
//! Wrong types are a catchable TypeError and a non-positive `token` length a
//! catchable ValueError — never a panic.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::error::{DogeError, DogeResult};
use crate::stdlib::int_arg;
use crate::value::Value;

/// A `Str` (its UTF-8 bytes) or `Bytes` argument as `&[u8]`, or a catchable type
/// error naming the member. Hashing treats text and raw bytes uniformly.
fn str_or_bytes<'a>(fname: &str, v: &'a Value) -> DogeResult<&'a [u8]> {
    match v {
        Value::Str(s) => Ok(s.as_bytes()),
        Value::Bytes(b) => Ok(b),
        _ => Err(DogeError::type_error(format!(
            "crypto.{fname} needs a Str or Bytes, got {}",
            v.describe()
        ))),
    }
}

pub fn crypto_sha256(data: &Value) -> DogeResult {
    let digest = Sha256::digest(str_or_bytes("sha256", data)?);
    Ok(Value::bytes(digest))
}

pub fn crypto_hmac_sha256(key: &Value, data: &Value) -> DogeResult {
    let key = str_or_bytes("hmac_sha256", key)?;
    let data = str_or_bytes("hmac_sha256", data)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(key)
        .map_err(|_| DogeError::value_error("crypto.hmac_sha256 could not build the HMAC key"))?;
    mac.update(data);
    Ok(Value::bytes(mac.finalize().into_bytes()))
}

pub fn crypto_token(n: &Value) -> DogeResult {
    let n = int_arg("crypto", "token", n)?;
    if n <= 0 {
        return Err(DogeError::value_error(format!(
            "crypto.token needs a positive length, got {n}"
        )));
    }
    let mut buf = vec![0u8; n as usize];
    getrandom::getrandom(&mut buf)
        .map_err(|_| DogeError::io_error("crypto.token could not read the OS random source"))?;
    Ok(Value::bytes(buf))
}

pub fn crypto_same(a: &Value, b: &Value) -> DogeResult {
    let a = str_or_bytes("same", a)?;
    let b = str_or_bytes("same", b)?;
    let equal = a.len() == b.len() && bool::from(a.ct_eq(b));
    Ok(Value::Bool(equal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn hex(v: &Value) -> String {
        match v {
            Value::Bytes(b) => b.iter().map(|byte| format!("{byte:02x}")).collect(),
            _ => panic!("expected Bytes"),
        }
    }

    fn as_bool(v: &Value) -> bool {
        match v {
            Value::Bool(b) => *b,
            _ => panic!("expected Bool"),
        }
    }

    #[test]
    fn sha256_matches_known_vector() {
        let empty = crypto_sha256(&Value::str("")).unwrap();
        assert_eq!(
            hex(&empty),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let abc = crypto_sha256(&Value::str("abc")).unwrap();
        assert_eq!(
            hex(&abc),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_str_and_bytes_agree() {
        let from_str = crypto_sha256(&Value::str("doge")).unwrap();
        let from_bytes = crypto_sha256(&Value::bytes(b"doge")).unwrap();
        assert_eq!(hex(&from_str), hex(&from_bytes));
    }

    #[test]
    fn hmac_sha256_matches_rfc4231_vector() {
        // RFC 4231 test case 2: key = "Jefe", data = "what do ya want for nothing?"
        let mac = crypto_hmac_sha256(
            &Value::str("Jefe"),
            &Value::str("what do ya want for nothing?"),
        )
        .unwrap();
        assert_eq!(
            hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn token_has_requested_length_and_varies() {
        let a = crypto_token(&Value::int(16)).unwrap();
        let b = crypto_token(&Value::int(16)).unwrap();
        match (&a, &b) {
            (Value::Bytes(x), Value::Bytes(y)) => {
                assert_eq!(x.len(), 16);
                assert_eq!(y.len(), 16);
                assert_ne!(x, y, "two secure draws must differ");
            }
            _ => panic!("expected Bytes"),
        }
    }

    #[test]
    fn token_rejects_non_positive_length() {
        assert_eq!(
            crypto_token(&Value::int(0)).unwrap_err().kind,
            ErrorKind::ValueError
        );
        assert_eq!(
            crypto_token(&Value::int(-1)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn same_compares_content_and_accepts_str_or_bytes() {
        assert!(as_bool(
            &crypto_same(&Value::str("secret"), &Value::str("secret")).unwrap()
        ));
        assert!(!as_bool(
            &crypto_same(&Value::str("secret"), &Value::str("secreT")).unwrap()
        ));
        assert!(!as_bool(
            &crypto_same(&Value::str("ab"), &Value::str("abc")).unwrap()
        ));
        assert!(as_bool(
            &crypto_same(&Value::str("hi"), &Value::bytes(b"hi")).unwrap()
        ));
    }

    #[test]
    fn wrong_types_are_type_errors() {
        assert_eq!(
            crypto_sha256(&Value::int(123)).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            crypto_hmac_sha256(&Value::int(1), &Value::str("x"))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            crypto_same(&Value::int(1), &Value::str("x"))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            crypto_token(&Value::str("16")).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
