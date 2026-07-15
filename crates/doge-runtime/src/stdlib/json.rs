//! `json` — parse and emit JSON. `json.parse(text)` turns a JSON document into a
//! Doge value (object → Dict, array → List, the rest → Str/Int/Float/Bool/none);
//! `json.emit(value)` turns one back into compact JSON text. Every malformed input
//! is a catchable `ValueError` with the offset it failed at, and a value that has
//! no JSON form (an object, function, socket, …) is a catchable `TypeError` —
//! neither ever panics.

use std::fmt::Write;

use num_bigint::BigInt;

use crate::error::{DogeError, DogeResult};
use crate::ordered_map::OrderedMap;
use crate::stdlib::serialize::{escape_str, too_deep, unsupported, MAX_DEPTH};
use crate::stdlib::str_arg;
use crate::value::Value;

/// `json.parse(text)` — the value a JSON document denotes. Leading and trailing
/// whitespace is ignored; anything after the top-level value is an error.
pub fn json_parse(text: &Value) -> DogeResult {
    let text = str_arg("json", "parse", text)?;
    let mut p = Parser {
        chars: text.chars().collect(),
        pos: 0,
    };
    p.skip_ws();
    let value = p.value(0)?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(p.error("expected end of input"));
    }
    Ok(value)
}

/// `json.emit(value)` — a compact JSON document (no insignificant whitespace) for
/// `value`. Dict/List/Str/Int/Float/Bool/none serialize; any other type, or a
/// non-finite Float (JSON has no NaN/infinity), is a catchable error.
pub fn json_emit(value: &Value) -> DogeResult {
    let mut out = String::new();
    emit(value, &mut out, 0)?;
    Ok(Value::str(out))
}

fn emit(value: &Value, out: &mut String, depth: usize) -> Result<(), DogeError> {
    if depth >= MAX_DEPTH {
        return Err(too_deep("json", "emit"));
    }
    match value {
        Value::None => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        // A Decimal emits as a bare JSON number, preserving its exact digits.
        // JSON has no decimal type, so a round-trip through `json.parse` returns a
        // Float — the value is exact on the wire, inexact only if re-parsed.
        Value::Decimal(d) => {
            let _ = write!(out, "{d}");
        }
        Value::Float(x) => {
            if !x.is_finite() {
                return Err(DogeError::value_error(
                    "json.emit cannot serialize a Float that is not finite",
                ));
            }
            // A whole-number Float keeps its decimal point so it re-parses as a
            // Float, matching how `bark` prints it (3.0, never 3).
            if x.fract() == 0.0 {
                let _ = write!(out, "{x:.1}");
            } else {
                let _ = write!(out, "{x}");
            }
        }
        Value::Str(s) => escape_str(s, out, |o, cp| {
            let _ = write!(o, "\\u{cp:04x}");
        }),
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.borrow().iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                emit(item, out, depth + 1)?;
            }
            out.push(']');
        }
        Value::Dict(entries) => {
            out.push('{');
            for (i, (key, val)) in entries.borrow().iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                escape_str(key, out, |o, cp| {
                    let _ = write!(o, "\\u{cp:04x}");
                });
                out.push(':');
                emit(val, out, depth + 1)?;
            }
            out.push('}');
        }
        other => return Err(unsupported("json", other)),
    }
    Ok(())
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn error(&self, what: &str) -> DogeError {
        DogeError::value_error(format!(
            "json.parse: much invalid. {what} at offset {}",
            self.pos
        ))
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.pos += 1;
        }
    }

    fn value(&mut self, depth: usize) -> DogeResult {
        if depth >= MAX_DEPTH {
            return Err(too_deep("json", "parse"));
        }
        match self.peek() {
            Some('{') => self.object(depth),
            Some('[') => self.array(depth),
            Some('"') => Ok(Value::str(self.string()?)),
            Some('t') => self.keyword("true", Value::Bool(true)),
            Some('f') => self.keyword("false", Value::Bool(false)),
            Some('n') => self.keyword("null", Value::None),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(_) => Err(self.error("unexpected character")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn keyword(&mut self, word: &str, value: Value) -> DogeResult {
        for expected in word.chars() {
            if self.peek() != Some(expected) {
                return Err(self.error("unexpected character"));
            }
            self.pos += 1;
        }
        Ok(value)
    }

    fn object(&mut self, depth: usize) -> DogeResult {
        self.pos += 1; // consume '{'
        let mut map = OrderedMap::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.pos += 1;
            return Ok(Value::dict(map));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some('"') {
                return Err(self.error("expected a string key"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(':') {
                return Err(self.error("expected ':'"));
            }
            self.pos += 1;
            self.skip_ws();
            let val = self.value(depth + 1)?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(',') => self.pos += 1,
                Some('}') => {
                    self.pos += 1;
                    return Ok(Value::dict(map));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> DogeResult {
        self.pos += 1; // consume '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.pos += 1;
            return Ok(Value::list(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value(depth + 1)?);
            self.skip_ws();
            match self.peek() {
                Some(',') => self.pos += 1,
                Some(']') => {
                    self.pos += 1;
                    return Ok(Value::list(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn string(&mut self) -> DogeResult<String> {
        self.pos += 1; // consume opening '"'
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated string")),
                Some('"') => {
                    self.pos += 1;
                    return Ok(s);
                }
                Some('\\') => {
                    self.pos += 1;
                    self.escape(&mut s)?;
                }
                Some(c) if (c as u32) < 0x20 => {
                    return Err(self.error("control character in a string"))
                }
                Some(c) => {
                    s.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn escape(&mut self, s: &mut String) -> DogeResult<()> {
        match self.peek() {
            Some('"') => s.push('"'),
            Some('\\') => s.push('\\'),
            Some('/') => s.push('/'),
            Some('b') => s.push('\u{08}'),
            Some('f') => s.push('\u{0c}'),
            Some('n') => s.push('\n'),
            Some('r') => s.push('\r'),
            Some('t') => s.push('\t'),
            Some('u') => {
                self.pos += 1;
                return self.unicode_escape(s);
            }
            _ => return Err(self.error("invalid escape")),
        }
        self.pos += 1;
        Ok(())
    }

    /// A `\uXXXX` escape (the leading `\u` already consumed). A high surrogate is
    /// paired with a following `\uXXXX` low surrogate into one code point, so
    /// astral characters written as surrogate pairs decode correctly.
    fn unicode_escape(&mut self, s: &mut String) -> DogeResult<()> {
        let hi = self.hex4()?;
        let cp = if (0xd800..=0xdbff).contains(&hi) {
            if self.peek() != Some('\\') {
                return Err(self.error("lone surrogate in a \\u escape"));
            }
            self.pos += 1;
            if self.peek() != Some('u') {
                return Err(self.error("lone surrogate in a \\u escape"));
            }
            self.pos += 1;
            let lo = self.hex4()?;
            if !(0xdc00..=0xdfff).contains(&lo) {
                return Err(self.error("invalid low surrogate in a \\u escape"));
            }
            0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
        } else if (0xdc00..=0xdfff).contains(&hi) {
            return Err(self.error("lone surrogate in a \\u escape"));
        } else {
            hi
        };
        match char::from_u32(cp) {
            Some(c) => {
                s.push(c);
                Ok(())
            }
            None => Err(self.error("invalid code point in a \\u escape")),
        }
    }

    fn hex4(&mut self) -> DogeResult<u32> {
        let mut value = 0u32;
        for _ in 0..4 {
            match self.peek().and_then(|c| c.to_digit(16)) {
                Some(d) => {
                    value = value * 16 + d;
                    self.pos += 1;
                }
                None => return Err(self.error("expected four hex digits after \\u")),
            }
        }
        Ok(value)
    }

    /// A JSON number. With neither a fraction nor an exponent it is an Int when it
    /// fits `i64` (otherwise a Float); with either, it is always a Float.
    fn number(&mut self) -> DogeResult {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        self.digits()?;
        let mut is_float = false;
        if self.peek() == Some('.') {
            is_float = true;
            self.pos += 1;
            self.digits()?;
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some('+' | '-')) {
                self.pos += 1;
            }
            self.digits()?;
        }
        let literal: String = self.chars[start..self.pos].iter().collect();
        if !is_float {
            if let Ok(n) = literal.parse::<BigInt>() {
                return Ok(Value::Int(n));
            }
        }
        match literal.parse::<f64>() {
            Ok(x) => Ok(Value::Float(x)),
            Err(_) => Err(self.error("invalid number")),
        }
    }

    fn digits(&mut self) -> DogeResult<()> {
        if !matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            return Err(self.error("expected a digit"));
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::ops::values_equal;

    fn parse(text: &str) -> DogeResult {
        json_parse(&Value::str(text))
    }

    fn emit_str(v: &Value) -> String {
        match json_emit(v).unwrap() {
            Value::Str(s) => s.to_string(),
            other => panic!("expected a Str, got {other:?}"),
        }
    }

    #[test]
    fn scalars_parse_to_the_right_types() {
        assert!(matches!(parse("null").unwrap(), Value::None));
        assert!(matches!(parse("true").unwrap(), Value::Bool(true)));
        assert!(matches!(parse("false").unwrap(), Value::Bool(false)));
        assert!(values_equal(&parse("  42 ").unwrap(), &Value::int(42)));
        assert!(values_equal(&parse("-7").unwrap(), &Value::int(-7)));
        assert!(matches!(parse("\"wow\"").unwrap(), Value::Str(s) if &*s == "wow"));
    }

    #[test]
    fn a_fraction_or_exponent_is_a_float_but_a_bare_integer_is_an_int() {
        assert!(matches!(parse("3.5").unwrap(), Value::Float(x) if x == 3.5));
        assert!(matches!(parse("3.0").unwrap(), Value::Float(x) if x == 3.0));
        assert!(matches!(parse("1e2").unwrap(), Value::Float(x) if x == 100.0));
        assert!(values_equal(&parse("100").unwrap(), &Value::int(100)));
    }

    #[test]
    fn an_integer_past_i64_parses_exactly() {
        // 2^63 does not fit i64, but Int is arbitrary precision, so it parses
        // exactly rather than losing precision to a Float.
        let big = "9223372036854775808";
        assert_eq!(parse(big).unwrap().to_string(), big);
    }

    #[test]
    fn nested_structure_round_trips_through_emit() {
        let src = "{\"name\":\"kabosu\",\"tags\":[\"doge\",\"shibe\"],\"age\":7,\"good\":true,\"note\":null}";
        let value = parse(src).unwrap();
        assert_eq!(emit_str(&value), src);
    }

    #[test]
    fn a_surrogate_pair_decodes_to_one_astral_character() {
        // U+1F415 DOG, written as a UTF-16 surrogate pair.
        assert!(matches!(parse("\"\\ud83d\\udc15\"").unwrap(), Value::Str(s) if &*s == "🐕"));
    }

    #[test]
    fn a_duplicate_key_keeps_the_last_value() {
        assert_eq!(emit_str(&parse("{\"a\":1,\"a\":2}").unwrap()), "{\"a\":2}");
    }

    #[test]
    fn emit_escapes_quotes_and_control_characters() {
        assert_eq!(
            emit_str(&Value::str("a\"b\n\t\u{01}")),
            "\"a\\\"b\\n\\t\\u0001\""
        );
    }

    #[test]
    fn trailing_garbage_is_a_catchable_value_error() {
        assert_eq!(parse("[1, 2] wow").unwrap_err().kind, ErrorKind::ValueError);
    }

    #[test]
    fn deep_nesting_is_a_catchable_error_not_a_stack_overflow() {
        let deep = "[".repeat(MAX_DEPTH + 5);
        assert_eq!(parse(&deep).unwrap_err().kind, ErrorKind::ValueError);
    }

    #[test]
    fn a_non_finite_float_cannot_be_emitted() {
        assert_eq!(
            json_emit(&Value::Float(f64::NAN)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn an_unsupported_type_is_a_catchable_type_error() {
        let f = Value::function(0, "greet", vec![]);
        assert_eq!(json_emit(&f).unwrap_err().kind, ErrorKind::TypeError);
    }

    #[test]
    fn a_non_str_argument_to_parse_is_a_type_error() {
        assert_eq!(
            json_parse(&Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
