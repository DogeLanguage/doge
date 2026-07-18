//! Hand-written base64 (RFC 4648, standard alphabet, `=` padding) and hex codecs
//! shared by `Bytes` and `Str` methods, keeping the dependency set minimal.
//!
//! Encoders are infallible; decoders return `Err(())` on any malformed input so the
//! calling method can raise a catchable `ValueError` (never a panic).

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with `=` padding.
pub(crate) fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(B64_ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Decode standard, padded base64. Rejects a bad length, a stray character,
/// misplaced padding, or padding before the final quartet.
pub(crate) fn b64_decode(s: &str) -> Result<Vec<u8>, ()> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if bytes.len() % 4 != 0 {
        return Err(());
    }
    let quartets = bytes.len() / 4;
    let mut out = Vec::with_capacity(quartets * 3);
    for (i, quartet) in bytes.chunks(4).enumerate() {
        let last = i + 1 == quartets;
        let pad2 = quartet[2] == b'=';
        let pad3 = quartet[3] == b'=';
        // Padding is confined to the final quartet; third-slot `=` requires a fourth-slot `=`.
        if (pad2 && !pad3) || (pad3 && !last) {
            return Err(());
        }
        let c0 = b64_value(quartet[0])?;
        let c1 = b64_value(quartet[1])?;
        let c2 = if pad2 { 0 } else { b64_value(quartet[2])? };
        let c3 = if pad3 { 0 } else { b64_value(quartet[3])? };
        let triple = ((c0 as u32) << 18) | ((c1 as u32) << 12) | ((c2 as u32) << 6) | (c3 as u32);
        out.push(((triple >> 16) & 0xff) as u8);
        if !pad2 {
            out.push(((triple >> 8) & 0xff) as u8);
        }
        if !pad3 {
            out.push((triple & 0xff) as u8);
        }
    }
    Ok(out)
}

/// Decode lowercase or uppercase hex. Rejects an odd length or a non-hex digit.
pub(crate) fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
    let bytes = s.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(());
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks(2) {
        out.push((hex_value(pair[0])? << 4) | hex_value(pair[1])?);
    }
    Ok(out)
}

fn b64_value(c: u8) -> Result<u8, ()> {
    match c {
        b'A'..=b'Z' => Ok(c - b'A'),
        b'a'..=b'z' => Ok(c - b'a' + 26),
        b'0'..=b'9' => Ok(c - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(()),
    }
}

fn hex_value(c: u8) -> Result<u8, ()> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_known_vectors() {
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"hi"), "aGk=");
        assert_eq!(b64_encode("héllo".as_bytes()), "aMOpbGxv");
    }

    #[test]
    fn b64_round_trips_every_length() {
        for len in 0..64usize {
            let data: Vec<u8> = (0..len).map(|n| (n * 7 % 256) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&data)).unwrap(), data);
        }
    }

    #[test]
    fn b64_rejects_malformed() {
        assert!(b64_decode("aGk").is_err()); // length not a multiple of 4
        assert!(b64_decode("aG!=").is_err()); // stray character
        assert!(b64_decode("a=k=").is_err()); // padding before the final quartet
        assert!(b64_decode("aGk=aGk=").is_err()); // padded interior quartet
        assert_eq!(b64_decode("Zm9vYmE=").unwrap(), b"fooba"); // interior quartet + padded tail
        assert!(b64_decode("=aGk").is_err()); // leading padding
    }

    #[test]
    fn hex_round_trips_and_rejects() {
        assert_eq!(hex_decode("6869").unwrap(), b"hi");
        assert_eq!(hex_decode("00FF").unwrap(), vec![0x00, 0xff]);
        assert_eq!(hex_decode("").unwrap(), Vec::<u8>::new());
        assert!(hex_decode("abc").is_err()); // odd length
        assert!(hex_decode("zz").is_err()); // non-hex digit
    }
}
