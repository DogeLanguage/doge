//! `dson` — parse and emit DSON, Doge Serialized Object Notation
//! (<https://dogeon.xyz/>): JSON's shape wearing doge-speak. Objects are
//! `such … wow` with `is` between key and value, arrays are `so … many`, the
//! literals are `yes`/`no`/`empty`, and every number is written in octal. The
//! value mapping and catchable-error contract match the [`json`](super::json)
//! codec — object → Dict, array → List, the rest → Str/Int/Float/Bool/none — so
//! the two are interchangeable but for their surface syntax.

use std::fmt::Write;

use crate::error::{DogeError, DogeResult};
use crate::ordered_map::OrderedMap;
use crate::stdlib::serialize::{escape_str, too_deep, unsupported, MAX_DEPTH};
use crate::stdlib::str_arg;
use crate::value::Value;

/// The longest exact base-8 fraction a finite `f64` can have: a subnormal's
/// fraction is a dyadic rational with up to 1074 bits, and each octal digit
/// carries three bits, so the expansion terminates within `ceil(1074 / 3)`
/// digits. Used only as a safety bound — a correct expansion always stops sooner.
const OCTAL_FRACTION_LIMIT: usize = 358;

/// `dson.parse(text)` — the value a DSON document denotes. Leading and trailing
/// whitespace is ignored; anything after the top-level value is an error.
pub fn dson_parse(text: &Value) -> DogeResult {
    let text = str_arg("dson", "parse", text)?;
    let chars: Vec<char> = text.chars().collect();
    let (toks, offs) = lex(&chars)?;
    let mut p = Parser {
        toks,
        offs,
        end: chars.len(),
        pos: 0,
    };
    let value = p.value(0)?;
    if p.pos != p.toks.len() {
        return Err(p.error("expected end of input"));
    }
    Ok(value)
}

/// `dson.emit(value)` — a DSON document for `value`. Dict/List/Str/Int/Float/Bool/
/// none serialize; any other type, or a non-finite Float, is a catchable error.
pub fn dson_emit(value: &Value) -> DogeResult {
    let mut out = String::new();
    emit(value, &mut out, 0)?;
    Ok(Value::str(out))
}

fn dson_unicode(out: &mut String, cp: u32) {
    let _ = write!(out, "\\u{cp:06o}");
}

fn emit(value: &Value, out: &mut String, depth: usize) -> Result<(), DogeError> {
    if depth >= MAX_DEPTH {
        return Err(too_deep("dson", "emit"));
    }
    match value {
        Value::None => out.push_str("empty"),
        Value::Bool(b) => out.push_str(if *b { "yes" } else { "no" }),
        Value::Int(n) => emit_int(*n, out),
        Value::Float(x) => emit_float(*x, out)?,
        Value::Str(s) => escape_str(s, out, dson_unicode),
        Value::List(items) => {
            let items = items.borrow();
            if items.is_empty() {
                out.push_str("so many");
            } else {
                out.push_str("so ");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(" and ");
                    }
                    emit(item, out, depth + 1)?;
                }
                out.push_str(" many");
            }
        }
        Value::Dict(entries) => {
            let entries = entries.borrow();
            if entries.is_empty() {
                out.push_str("such wow");
            } else {
                out.push_str("such ");
                for (i, (key, val)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    escape_str(key, out, dson_unicode);
                    out.push_str(" is ");
                    emit(val, out, depth + 1)?;
                }
                out.push_str(" wow");
            }
        }
        other => return Err(unsupported("dson", other)),
    }
    Ok(())
}

/// An Int as signed octal (`-` then the magnitude's octal digits).
fn emit_int(n: i64, out: &mut String) {
    if n < 0 {
        let _ = write!(out, "-{:o}", n.unsigned_abs());
    } else {
        let _ = write!(out, "{n:o}");
    }
}

/// A finite Float as its exact octal expansion, always with a fractional part
/// (`0.4` for 0.5) so it re-parses as a Float rather than an Int. NaN/±infinity
/// have no DSON form and are a catchable error.
fn emit_float(x: f64, out: &mut String) -> Result<(), DogeError> {
    if !x.is_finite() {
        return Err(DogeError::value_error(
            "dson.emit cannot serialize a Float that is not finite",
        ));
    }
    if x.is_sign_negative() && x != 0.0 {
        out.push('-');
    }
    let mut v = x.abs();
    let int_part = v.trunc();
    v -= int_part;

    // Integer part in octal, most-significant digit first.
    if int_part < 1.0 {
        out.push('0');
    } else {
        let mut digits = Vec::new();
        let mut ip = int_part;
        while ip >= 1.0 {
            digits.push(octal_digit((ip % 8.0) as u32));
            ip = (ip / 8.0).floor();
        }
        out.extend(digits.iter().rev());
    }

    // Fractional part: multiply by 8 and take the whole part each step. Every
    // finite f64 fraction is a dyadic rational and 8 = 2^3, so this terminates.
    out.push('.');
    if v == 0.0 {
        out.push('0');
        return Ok(());
    }
    let mut produced = 0;
    while v != 0.0 && produced < OCTAL_FRACTION_LIMIT {
        v *= 8.0;
        let digit = v.floor();
        out.push(octal_digit(digit as u32));
        v -= digit;
        produced += 1;
    }
    Ok(())
}

fn octal_digit(d: u32) -> char {
    char::from_digit(d, 8).unwrap_or('0')
}

/// A DSON token. Numbers are resolved to a `Value` at lex time (octal is a
/// lexical concern); the structural words and separators carry no payload.
#[derive(Debug, Clone)]
enum Tok {
    Such,
    Wow,
    So,
    Many,
    Is,
    And,
    Also,
    Yes,
    No,
    Empty,
    /// Any of the pair separators `,` `.` `!` `?`.
    Sep,
    Str(String),
    Num(Value),
}

fn lex(chars: &[char]) -> DogeResult<(Vec<Tok>, Vec<usize>)> {
    let mut lexer = Lexer { chars, pos: 0 };
    let mut toks = Vec::new();
    let mut offs = Vec::new();
    loop {
        lexer.skip_ws();
        let off = lexer.pos;
        let Some(c) = lexer.peek() else { break };
        let tok = match c {
            '"' => Tok::Str(lexer.string()?),
            '-' => lexer.number()?,
            c if c.is_ascii_digit() => lexer.number()?,
            ',' | '.' | '!' | '?' => {
                lexer.pos += 1;
                Tok::Sep
            }
            c if c.is_ascii_alphabetic() => lexer.word()?,
            _ => return Err(lexer.error("unexpected character")),
        };
        toks.push(tok);
        offs.push(off);
    }
    Ok((toks, offs))
}

struct Lexer<'a> {
    chars: &'a [char],
    pos: usize,
}

impl Lexer<'_> {
    fn error(&self, what: &str) -> DogeError {
        DogeError::value_error(format!(
            "dson.parse: much invalid. {what} at offset {}",
            self.pos
        ))
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.pos + ahead).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.pos += 1;
        }
    }

    fn starts_with(&self, word: &str) -> bool {
        word.chars()
            .enumerate()
            .all(|(i, ch)| self.peek_at(i) == Some(ch))
    }

    fn word(&mut self) -> DogeResult<Tok> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        match word.as_str() {
            "such" => Ok(Tok::Such),
            "wow" => Ok(Tok::Wow),
            "so" => Ok(Tok::So),
            "many" => Ok(Tok::Many),
            "is" => Ok(Tok::Is),
            "and" => Ok(Tok::And),
            "also" => Ok(Tok::Also),
            "yes" => Ok(Tok::Yes),
            "no" => Ok(Tok::No),
            "empty" => Ok(Tok::Empty),
            _ => Err(DogeError::value_error(format!(
                "dson.parse: much invalid. unexpected word '{word}' at offset {start}"
            ))),
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

    /// A `\uOOOOOO` escape — six octal digits naming one code point (the leading
    /// `\u` already consumed).
    fn unicode_escape(&mut self, s: &mut String) -> DogeResult<()> {
        let mut cp = 0u32;
        for _ in 0..6 {
            match self.peek().and_then(|c| c.to_digit(8)) {
                Some(d) => {
                    cp = cp * 8 + d;
                    self.pos += 1;
                }
                None => return Err(self.error("expected six octal digits after \\u")),
            }
        }
        match char::from_u32(cp) {
            Some(c) => {
                s.push(c);
                Ok(())
            }
            None => Err(self.error("invalid code point in a \\u escape")),
        }
    }

    /// A DSON number in octal: `-?` octal digits, an optional `.`-fraction, and an
    /// optional `very`/`VERY` exponent (meaning × 8^n). Integral, fraction-free,
    /// exponent-free, and in `i64` range → Int; anything else → Float.
    fn number(&mut self) -> DogeResult<Tok> {
        let neg = self.peek() == Some('-');
        if neg {
            self.pos += 1;
        }
        let mut int_digits = Vec::new();
        self.octal_digits(&mut int_digits);
        if int_digits.is_empty() {
            return Err(self.error("expected an octal digit"));
        }
        if matches!(self.peek(), Some('8' | '9')) {
            return Err(self.error("not an octal digit (DSON numbers are base 8)"));
        }

        let mut is_float = false;
        let mut frac_digits = Vec::new();
        if self.peek() == Some('.') && matches!(self.peek_at(1), Some('0'..='7')) {
            is_float = true;
            self.pos += 1; // consume '.'
            self.octal_digits(&mut frac_digits);
        }

        let mut exponent = 0i32;
        if self.starts_with("very") || self.starts_with("VERY") {
            is_float = true;
            self.pos += 4;
            let esign = match self.peek() {
                Some('+') => {
                    self.pos += 1;
                    1
                }
                Some('-') => {
                    self.pos += 1;
                    -1
                }
                _ => 1,
            };
            let mut exp_digits = Vec::new();
            self.octal_digits(&mut exp_digits);
            if exp_digits.is_empty() {
                return Err(self.error("expected an octal digit in the exponent"));
            }
            let magnitude = exp_digits.iter().fold(0i32, |acc, &d| acc * 8 + d as i32);
            exponent = esign * magnitude;
        }

        if !is_float {
            if let Some(n) = octal_to_i64(&int_digits, neg) {
                return Ok(Tok::Num(Value::Int(n)));
            }
        }
        Ok(Tok::Num(Value::Float(octal_to_f64(
            &int_digits,
            &frac_digits,
            exponent,
            neg,
        ))))
    }

    fn octal_digits(&mut self, out: &mut Vec<u32>) {
        while let Some(c) = self.peek() {
            match c.to_digit(8) {
                Some(d) => {
                    out.push(d);
                    self.pos += 1;
                }
                None => break,
            }
        }
    }
}

/// Fold octal digits into a signed `i64`, or `None` if they overflow its range.
fn octal_to_i64(digits: &[u32], neg: bool) -> Option<i64> {
    let mut acc: i128 = 0;
    for &d in digits {
        acc = acc.checked_mul(8)?.checked_add(d as i128)?;
    }
    let signed = if neg { -acc } else { acc };
    i64::try_from(signed).ok()
}

/// Combine octal integer digits, octal fraction digits, and a base-8 exponent
/// into an `f64`.
fn octal_to_f64(int_digits: &[u32], frac_digits: &[u32], exponent: i32, neg: bool) -> f64 {
    let mut mag = int_digits
        .iter()
        .fold(0.0f64, |acc, &d| acc * 8.0 + d as f64);
    if !frac_digits.is_empty() {
        let frac = frac_digits
            .iter()
            .fold(0.0f64, |acc, &d| acc * 8.0 + d as f64);
        mag += frac / 8f64.powi(frac_digits.len() as i32);
    }
    if exponent != 0 {
        mag *= 8f64.powi(exponent);
    }
    if neg {
        -mag
    } else {
        mag
    }
}

struct Parser {
    toks: Vec<Tok>,
    offs: Vec<usize>,
    end: usize,
    pos: usize,
}

impl Parser {
    fn error(&self, what: &str) -> DogeError {
        let off = self.offs.get(self.pos).copied().unwrap_or(self.end);
        DogeError::value_error(format!("dson.parse: much invalid. {what} at offset {off}"))
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn value(&mut self, depth: usize) -> DogeResult {
        if depth >= MAX_DEPTH {
            return Err(too_deep("dson", "parse"));
        }
        match self.peek() {
            Some(Tok::Such) => self.object(depth),
            Some(Tok::So) => self.array(depth),
            Some(Tok::Str(s)) => {
                let s = s.clone();
                self.pos += 1;
                Ok(Value::str(s))
            }
            Some(Tok::Num(v)) => {
                let v = v.clone();
                self.pos += 1;
                Ok(v)
            }
            Some(Tok::Yes) => {
                self.pos += 1;
                Ok(Value::Bool(true))
            }
            Some(Tok::No) => {
                self.pos += 1;
                Ok(Value::Bool(false))
            }
            Some(Tok::Empty) => {
                self.pos += 1;
                Ok(Value::None)
            }
            _ => Err(self.error("expected a value")),
        }
    }

    fn object(&mut self, depth: usize) -> DogeResult {
        self.pos += 1; // consume 'such'
        let mut map = OrderedMap::new();
        if matches!(self.peek(), Some(Tok::Wow)) {
            self.pos += 1;
            return Ok(Value::dict(map));
        }
        loop {
            let key = match self.peek() {
                Some(Tok::Str(s)) => {
                    let s = s.clone();
                    self.pos += 1;
                    s
                }
                _ => return Err(self.error("expected a string key")),
            };
            if !matches!(self.peek(), Some(Tok::Is)) {
                return Err(self.error("expected 'is'"));
            }
            self.pos += 1;
            let val = self.value(depth + 1)?;
            map.insert(key, val);
            match self.peek() {
                Some(Tok::Sep) => self.pos += 1,
                Some(Tok::Wow) => {
                    self.pos += 1;
                    return Ok(Value::dict(map));
                }
                _ => return Err(self.error("expected a separator or 'wow'")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> DogeResult {
        self.pos += 1; // consume 'so'
        let mut items = Vec::new();
        if matches!(self.peek(), Some(Tok::Many)) {
            self.pos += 1;
            return Ok(Value::list(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            match self.peek() {
                Some(Tok::And | Tok::Also) => self.pos += 1,
                Some(Tok::Many) => {
                    self.pos += 1;
                    return Ok(Value::list(items));
                }
                _ => return Err(self.error("expected 'and', 'also', or 'many'")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn parse(text: &str) -> DogeResult {
        dson_parse(&Value::str(text))
    }

    fn emit_str(v: &Value) -> String {
        match dson_emit(v).unwrap() {
            Value::Str(s) => s.to_string(),
            other => panic!("expected a Str, got {other:?}"),
        }
    }

    #[test]
    fn literals_map_to_doge_values() {
        assert!(matches!(parse("yes").unwrap(), Value::Bool(true)));
        assert!(matches!(parse("no").unwrap(), Value::Bool(false)));
        assert!(matches!(parse("empty").unwrap(), Value::None));
    }

    #[test]
    fn numbers_are_octal() {
        assert!(matches!(parse("17620").unwrap(), Value::Int(8080)));
        assert!(matches!(parse("-12").unwrap(), Value::Int(-10)));
        // 4very2 = 4 * 8^2 = 256.
        assert!(matches!(parse("4very2").unwrap(), Value::Float(x) if x == 256.0));
        // 1very-1 = 1 * 8^-1 = 0.125.
        assert!(matches!(parse("1very-1").unwrap(), Value::Float(x) if x == 0.125));
    }

    #[test]
    fn octal_fractions_round_trip() {
        // 0.5 == 4/8 == octal 0.4; a plain integer emits without a point.
        assert!(matches!(parse("0.4").unwrap(), Value::Float(x) if x == 0.5));
        assert_eq!(emit_str(&Value::Float(0.5)), "0.4");
        assert_eq!(emit_str(&Value::Float(2.5)), "2.4");
        assert_eq!(emit_str(&Value::Int(8080)), "17620");
    }

    #[test]
    fn a_whole_float_keeps_its_point_so_it_stays_a_float() {
        assert_eq!(emit_str(&Value::Float(3.0)), "3.0");
        assert!(matches!(parse("3.0").unwrap(), Value::Float(x) if x == 3.0));
    }

    #[test]
    fn all_pair_separators_and_array_joiners_are_accepted() {
        let d =
            parse("such \"a\" is 1, \"b\" is 2. \"c\" is 3! \"d\" is 4? \"e\" is 5 wow").unwrap();
        assert_eq!(
            emit_str(&d),
            "such \"a\" is 1, \"b\" is 2, \"c\" is 3, \"d\" is 4, \"e\" is 5 wow"
        );
        let a = parse("so 1 and 2 also 3 many").unwrap();
        assert_eq!(emit_str(&a), "so 1 and 2 and 3 many");
    }

    #[test]
    fn empty_containers_round_trip() {
        assert_eq!(emit_str(&parse("such wow").unwrap()), "such wow");
        assert_eq!(emit_str(&parse("so many").unwrap()), "so many");
    }

    #[test]
    fn nested_structure_round_trips() {
        let src = "such \"name\" is \"kabosu\", \"tags\" is so \"doge\" and \"shibe\" many, \"good\" is yes wow";
        assert_eq!(emit_str(&parse(src).unwrap()), src);
    }

    #[test]
    fn six_octal_digit_unicode_escape_decodes() {
        // U+1F415 DOG == octal 372025.
        assert!(matches!(parse("\"\\u372025\"").unwrap(), Value::Str(s) if &*s == "🐕"));
    }

    #[test]
    fn a_non_octal_digit_is_a_catchable_error() {
        assert_eq!(parse("18").unwrap_err().kind, ErrorKind::ValueError);
    }

    #[test]
    fn trailing_garbage_is_a_catchable_error() {
        assert_eq!(
            parse("so 1 many wow").unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn deep_nesting_is_a_catchable_error_not_a_stack_overflow() {
        let deep = "so ".repeat(MAX_DEPTH + 5);
        assert_eq!(parse(&deep).unwrap_err().kind, ErrorKind::ValueError);
    }

    #[test]
    fn a_non_finite_float_cannot_be_emitted() {
        assert_eq!(
            dson_emit(&Value::Float(f64::INFINITY)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn an_unsupported_type_is_a_catchable_type_error() {
        let sock = Value::function(0, "f", vec![]);
        assert_eq!(dson_emit(&sock).unwrap_err().kind, ErrorKind::TypeError);
    }

    #[test]
    fn a_non_str_argument_to_parse_is_a_type_error() {
        assert_eq!(
            dson_parse(&Value::Int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
