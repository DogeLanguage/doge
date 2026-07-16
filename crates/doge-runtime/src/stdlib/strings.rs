use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// A Str argument as `&str`, or a catchable type error naming the `strings`
/// member. Thin wrapper over the shared [`crate::stdlib::str_arg`].
fn str_arg<'a>(fname: &str, v: &'a Value) -> DogeResult<&'a str> {
    crate::stdlib::str_arg("strings", fname, v)
}

/// `strings.beeg(s)` — every letter uppercased.
pub fn strings_beeg(s: &Value) -> DogeResult {
    Ok(Value::str(str_arg("beeg", s)?.to_uppercase()))
}

/// `strings.smoll(s)` — every letter lowercased.
pub fn strings_smoll(s: &Value) -> DogeResult {
    Ok(Value::str(str_arg("smoll", s)?.to_lowercase()))
}

/// `strings.trim(s)` — leading and trailing whitespace removed.
pub fn strings_trim(s: &Value) -> DogeResult {
    Ok(Value::str(str_arg("trim", s)?.trim()))
}

/// `strings.split(s, sep)` — a List of the pieces of `s` between each `sep`.
/// Empty pieces are kept (`"a,,b"` splits into three); splitting on an empty
/// separator is a catchable ValueError.
pub fn strings_split(s: &Value, sep: &Value) -> DogeResult {
    let s = str_arg("split", s)?;
    let sep = str_arg("split", sep)?;
    if sep.is_empty() {
        return Err(DogeError::value_error("cannot split on an empty Str"));
    }
    Ok(Value::list(s.split(sep).map(Value::str).collect()))
}

/// `strings.join(parts, sep)` — the Strs in `parts` joined with `sep`. Every
/// element of `parts` must be a Str.
pub fn strings_join(parts: &Value, sep: &Value) -> DogeResult {
    let items = match parts {
        Value::List(items) => items.borrow(),
        _ => return Err(DogeError::type_error("strings.join needs a List of Str")),
    };
    let mut pieces = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Str(piece) => pieces.push(piece.to_string()),
            _ => return Err(DogeError::type_error("strings.join needs a List of Str")),
        }
    }
    let sep = str_arg("join", sep)?;
    Ok(Value::str(pieces.join(sep)))
}

/// `strings.contains(s, needle)` — whether `needle` occurs anywhere in `s`.
pub fn strings_contains(s: &Value, needle: &Value) -> DogeResult {
    let s = str_arg("contains", s)?;
    let needle = str_arg("contains", needle)?;
    Ok(Value::Bool(s.contains(needle)))
}

/// `strings.index(s, sub)` — the character offset of the first `sub` in `s`, or
/// -1 when absent. Char-based like every `Str` operation: `str::find` returns a
/// byte position, so it is converted to a character count.
pub fn strings_index(s: &Value, sub: &Value) -> DogeResult {
    let s = str_arg("index", s)?;
    let sub = str_arg("index", sub)?;
    let offset = match s.find(sub) {
        Some(byte_pos) => s[..byte_pos].chars().count() as i64,
        None => -1,
    };
    Ok(Value::int(offset))
}

/// `strings.replace(s, from, to)` — every occurrence of `from` in `s` swapped
/// for `to`. Replacing an empty `from` is a catchable ValueError.
pub fn strings_replace(s: &Value, from: &Value, to: &Value) -> DogeResult {
    let s = str_arg("replace", s)?;
    let from = str_arg("replace", from)?;
    let to = str_arg("replace", to)?;
    if from.is_empty() {
        return Err(DogeError::value_error("cannot replace an empty Str"));
    }
    Ok(Value::str(s.replace(from, to)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn case_and_trim() {
        assert!(matches!(strings_beeg(&Value::str("wow")).unwrap(), Value::Str(s) if &*s == "WOW"));
        assert!(
            matches!(strings_smoll(&Value::str("WOW")).unwrap(), Value::Str(s) if &*s == "wow")
        );
        assert!(
            matches!(strings_trim(&Value::str("  hi  ")).unwrap(), Value::Str(s) if &*s == "hi")
        );
    }

    #[test]
    fn split_keeps_empty_pieces_and_rejects_empty_sep() {
        let parts = strings_split(&Value::str("a,,b"), &Value::str(",")).unwrap();
        match parts {
            Value::List(items) => assert_eq!(items.borrow().len(), 3),
            _ => panic!("expected a list"),
        }
        assert_eq!(
            strings_split(&Value::str("a"), &Value::str(""))
                .unwrap_err()
                .kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn join_needs_all_strs() {
        let parts = Value::list(vec![Value::str("much"), Value::str("wow")]);
        assert!(
            matches!(strings_join(&parts, &Value::str(" ")).unwrap(), Value::Str(s) if &*s == "much wow")
        );
        let mixed = Value::list(vec![Value::str("a"), Value::int(1)]);
        assert_eq!(
            strings_join(&mixed, &Value::str(" ")).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn contains_and_replace() {
        assert!(matches!(
            strings_contains(&Value::str("kabosu"), &Value::str("bos")).unwrap(),
            Value::Bool(true)
        ));
        assert!(
            matches!(strings_replace(&Value::str("a-b-c"), &Value::str("-"), &Value::str("_")).unwrap(), Value::Str(s) if &*s == "a_b_c")
        );
        assert_eq!(
            strings_replace(&Value::str("x"), &Value::str(""), &Value::str("y"))
                .unwrap_err()
                .kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn index_returns_char_offset_or_minus_one() {
        use bigdecimal::ToPrimitive;
        let at = |s, sub| match strings_index(&Value::str(s), &Value::str(sub)).unwrap() {
            Value::Int(n) => n.to_i64().unwrap(),
            _ => panic!("expected an Int"),
        };
        assert_eq!(at("kabosu", "bos"), 2);
        assert_eq!(at("kabosu", "zzz"), -1);
        // Char-based, not byte-based: the accented char is two bytes but one char.
        assert_eq!(at("héllo", "llo"), 2);
    }

    #[test]
    fn non_str_subject_is_a_type_error() {
        assert_eq!(
            strings_trim(&Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
