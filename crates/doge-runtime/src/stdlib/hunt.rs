//! `hunt` — regular expressions. The dog chases a pattern through a Str: `test`
//! asks whether it matches, `find`/`find_all` return the matching text, `groups`
//! pulls out capture groups, and `replace` swaps every match for a replacement.
//!
//! Every member returns matched *substrings* (Str) and Lists of Str, never byte
//! offsets, so the char-vs-byte indexing guarantee never leaks to the user. An
//! invalid pattern is a catchable `ValueError`; a no-match lookup reads back as
//! `none`. Backed by the `regex` crate, whose linear-time engine cannot blow up on
//! a pathological pattern — matching the runtime's never-panics guarantee.

use regex::Regex;

use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// A Str argument as `&str`, or a catchable type error naming the `hunt` member.
/// Thin wrapper over the shared [`crate::stdlib::str_arg`].
fn str_arg<'a>(fname: &str, v: &'a Value) -> DogeResult<&'a str> {
    crate::stdlib::str_arg("hunt", fname, v)
}

/// Compile `pat` into a [`Regex`], or a catchable `ValueError` describing the flaw.
/// The raw `regex::Error` never reaches the user — only the pattern and a hint.
fn compile(fname: &str, pat: &Value) -> Result<Regex, DogeError> {
    let pat = str_arg(fname, pat)?;
    Regex::new(pat).map_err(|_| DogeError::value_error(format!("not a valid pattern: \"{pat}\"")))
}

/// `hunt.test(pat, text)` — whether `pat` matches anywhere in `text`.
pub fn hunt_test(pat: &Value, text: &Value) -> DogeResult {
    let re = compile("test", pat)?;
    let text = str_arg("test", text)?;
    Ok(Value::Bool(re.is_match(text)))
}

/// `hunt.find(pat, text)` — the first substring of `text` that matches `pat`, or
/// `none` when there is no match.
pub fn hunt_find(pat: &Value, text: &Value) -> DogeResult {
    let re = compile("find", pat)?;
    let text = str_arg("find", text)?;
    Ok(match re.find(text) {
        Some(m) => Value::str(m.as_str()),
        None => Value::None,
    })
}

/// `hunt.find_all(pat, text)` — a List of every non-overlapping match of `pat` in
/// `text`, in order; an empty List when there is none.
pub fn hunt_find_all(pat: &Value, text: &Value) -> DogeResult {
    let re = compile("find_all", pat)?;
    let text = str_arg("find_all", text)?;
    Ok(Value::list(
        re.find_iter(text).map(|m| Value::str(m.as_str())).collect(),
    ))
}

/// `hunt.groups(pat, text)` — the capture groups of the first match, group 0 (the
/// whole match) first; a group that did not participate is `none`. `none` overall
/// when `pat` does not match.
pub fn hunt_groups(pat: &Value, text: &Value) -> DogeResult {
    let re = compile("groups", pat)?;
    let text = str_arg("groups", text)?;
    Ok(match re.captures(text) {
        Some(caps) => Value::list(
            caps.iter()
                .map(|g| match g {
                    Some(m) => Value::str(m.as_str()),
                    None => Value::None,
                })
                .collect(),
        ),
        None => Value::None,
    })
}

/// `hunt.replace(pat, text, repl)` — every match of `pat` in `text` swapped for
/// `repl`, where `repl` may reference capture groups as `$1` or `${name}`.
pub fn hunt_replace(pat: &Value, text: &Value, repl: &Value) -> DogeResult {
    let re = compile("replace", pat)?;
    let text = str_arg("replace", text)?;
    let repl = str_arg("replace", repl)?;
    Ok(Value::str(re.replace_all(text, repl)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn s(v: &str) -> Value {
        Value::str(v)
    }

    #[test]
    fn test_matches_anywhere() {
        assert!(matches!(
            hunt_test(&s("[0-9]+"), &s("order 42")).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            hunt_test(&s("^woof"), &s("bark")).unwrap(),
            Value::Bool(false)
        ));
    }

    #[test]
    fn find_returns_first_match_or_none() {
        assert!(
            matches!(hunt_find(&s("[0-9]+"), &s("order 42")).unwrap(), Value::Str(m) if &*m == "42")
        );
        assert!(matches!(
            hunt_find(&s("[0-9]+"), &s("no digits")).unwrap(),
            Value::None
        ));
    }

    #[test]
    fn find_all_collects_every_match() {
        match hunt_find_all(&s("[0-9]+"), &s("1 and 22 and 333")).unwrap() {
            Value::List(items) => {
                let items = items.borrow();
                assert_eq!(items.len(), 3);
                assert!(matches!(&items[2], Value::Str(m) if &**m == "333"));
            }
            _ => panic!("expected a list"),
        }
        match hunt_find_all(&s("[0-9]+"), &s("none here")).unwrap() {
            Value::List(items) => assert!(items.borrow().is_empty()),
            _ => panic!("expected a list"),
        }
    }

    #[test]
    fn groups_returns_whole_match_then_captures() {
        match hunt_groups(&s("(\\w+)@(\\w+)"), &s("doge@shibe")).unwrap() {
            Value::List(items) => {
                let items = items.borrow();
                assert_eq!(items.len(), 3);
                assert!(matches!(&items[0], Value::Str(m) if &**m == "doge@shibe"));
                assert!(matches!(&items[1], Value::Str(m) if &**m == "doge"));
                assert!(matches!(&items[2], Value::Str(m) if &**m == "shibe"));
            }
            _ => panic!("expected a list"),
        }
        assert!(matches!(
            hunt_groups(&s("(\\w+)@(\\w+)"), &s("no at sign")).unwrap(),
            Value::None
        ));
    }

    #[test]
    fn groups_non_participating_is_none() {
        match hunt_groups(&s("(a)|(b)"), &s("a")).unwrap() {
            Value::List(items) => {
                let items = items.borrow();
                assert!(matches!(&items[1], Value::Str(m) if &**m == "a"));
                assert!(matches!(&items[2], Value::None));
            }
            _ => panic!("expected a list"),
        }
    }

    #[test]
    fn replace_swaps_every_match_with_backrefs() {
        assert!(matches!(
            hunt_replace(&s("[0-9]+"), &s("a1b22c"), &s("#")).unwrap(),
            Value::Str(m) if &*m == "a#b#c"
        ));
        assert!(matches!(
            hunt_replace(&s("(\\w+)@(\\w+)"), &s("doge@shibe"), &s("$2.$1")).unwrap(),
            Value::Str(m) if &*m == "shibe.doge"
        ));
    }

    #[test]
    fn invalid_pattern_is_a_value_error() {
        assert_eq!(
            hunt_find(&s("["), &s("x")).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn non_str_argument_is_a_type_error() {
        assert_eq!(
            hunt_test(&Value::Int(1), &s("x")).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            hunt_find(&s("x"), &Value::Int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
