use std::fmt;
use std::rc::Rc;

use crate::value::Value;

/// The deepest a `bonk`-able call chain may nest before the runtime stops it
/// with a catchable [`ErrorKind::RecursionLimit`] error.
pub const RECURSION_LIMIT: usize = 1000;

/// The category of a runtime error. Each variant maps to a `pls`/`oh no`
/// catchable failure a Doge program can hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// An operator or builtin was handed a value of the wrong type.
    TypeError,
    /// `/` or `//` or `%` with a zero divisor.
    DivisionByZero,
    /// A number too large to materialize where a bounded one is unavoidable — a
    /// `**` exponent too big to compute, sequence repetition too large to
    /// materialize, or a non-finite Float narrowed to an Int. Ordinary Int
    /// arithmetic is arbitrary precision and never overflows.
    Overflow,
    /// List/Str index outside the valid range.
    IndexOutOfBounds,
    /// Dict lookup for a key that is not present.
    KeyError,
    /// A value was the right type but not a usable value (e.g. `int("dog")`).
    ValueError,
    /// An I/O or environment operation failed: a file could not be read/written,
    /// or held bytes that were not valid text.
    IOError,
    /// A missing field or method on an object, or a method call on a value whose
    /// type has no methods at all.
    AttrError,
    /// A `bonk` raised by the program itself.
    Bonk,
    /// An `amaze` assertion whose condition was falsy.
    AssertError,
    /// A call chain nested past [`RECURSION_LIMIT`].
    RecursionLimit,
}

impl ErrorKind {
    /// Short stable identifier, handy for tests and future diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::TypeError => "TypeError",
            ErrorKind::DivisionByZero => "DivisionByZero",
            ErrorKind::Overflow => "Overflow",
            ErrorKind::IndexOutOfBounds => "IndexOutOfBounds",
            ErrorKind::KeyError => "KeyError",
            ErrorKind::ValueError => "ValueError",
            ErrorKind::IOError => "IOError",
            ErrorKind::AttrError => "AttrError",
            ErrorKind::Bonk => "Bonk",
            ErrorKind::AssertError => "AssertError",
            ErrorKind::RecursionLimit => "RecursionLimit",
        }
    }
}

/// Where an error was raised: the script path and 1-based line it came from.
/// Present only on a re-raised error (`bonk err`), so its original location
/// survives instead of being overwritten by the `bonk` site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorLocation {
    pub file: Rc<str>,
    pub line: u32,
}

/// A catchable runtime error: a category plus a precise, plain-English message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DogeError {
    pub kind: ErrorKind,
    pub message: String,
    /// The raise site, carried only across `bonk err` so a re-raised error keeps
    /// its original location. `None` on a freshly built error — the catch site
    /// supplies the location when it becomes an [`error_value`].
    pub location: Option<ErrorLocation>,
}

impl DogeError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        DogeError {
            kind,
            message: message.into(),
            location: None,
        }
    }

    pub fn type_error(message: impl Into<String>) -> Self {
        DogeError::new(ErrorKind::TypeError, message)
    }

    pub fn division_by_zero(message: impl Into<String>) -> Self {
        DogeError::new(ErrorKind::DivisionByZero, message)
    }

    pub fn overflow(message: impl Into<String>) -> Self {
        DogeError::new(ErrorKind::Overflow, message)
    }

    pub fn index_out_of_bounds(message: impl Into<String>) -> Self {
        DogeError::new(ErrorKind::IndexOutOfBounds, message)
    }

    pub fn key_error(message: impl Into<String>) -> Self {
        DogeError::new(ErrorKind::KeyError, message)
    }

    pub fn value_error(message: impl Into<String>) -> Self {
        DogeError::new(ErrorKind::ValueError, message)
    }

    pub fn attr_error(message: impl Into<String>) -> Self {
        DogeError::new(ErrorKind::AttrError, message)
    }

    pub fn io_error(message: impl Into<String>) -> Self {
        DogeError::new(ErrorKind::IOError, message)
    }
}

impl fmt::Display for DogeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DogeError {}

/// The innards of a caught `Error` value: the category, message, and the raise
/// site, read through `err.type` / `err.message` / `err.file` / `err.line`.
#[derive(Debug)]
pub struct ErrorData {
    pub kind: ErrorKind,
    pub message: Rc<str>,
    pub file: Rc<str>,
    pub line: u32,
}

/// Build the error a `bonk <expr>` raises. Re-raising a caught `Error` value
/// (`bonk err`) preserves its type, message, and original location; any other
/// value raises a `Bonk` whose message is the value's display form, so `bonk 5`
/// reads `5` and `bonk "much fail"` reads `much fail` — the text `bark` prints.
pub fn bonk_error(value: &Value) -> DogeError {
    match value {
        Value::Error(e) => DogeError {
            kind: e.kind,
            message: e.message.to_string(),
            location: Some(ErrorLocation {
                file: e.file.clone(),
                line: e.line,
            }),
        },
        _ => DogeError::new(ErrorKind::Bonk, value.to_string()),
    }
}

/// The default message for an `amaze` assertion that fails without one of its own.
const ASSERT_DEFAULT_MESSAGE: &str = "such amaze. much false.";

/// Build the error a failing `amaze <cond>` raises. With a message
/// (`amaze cond, msg`) the message value's display form becomes the error text,
/// mirroring `bonk`; without one it takes the default doge-flavored line.
pub fn assert_error(message: Option<&Value>) -> DogeError {
    let text = match message {
        Some(value) => value.to_string(),
        None => ASSERT_DEFAULT_MESSAGE.to_string(),
    };
    DogeError::new(ErrorKind::AssertError, text)
}

/// The value bound by `oh no err!`: a structured `Error` carrying the caught
/// error's type, message, and location. A re-raised error keeps its embedded
/// location; a fresh one takes the catch site's `file`/`line`.
pub fn error_value(err: &DogeError, file: &str, line: u32) -> Value {
    let (file, line) = match &err.location {
        Some(loc) => (loc.file.clone(), loc.line),
        None => (Rc::from(file), line),
    };
    Value::error(err.kind, &err.message, file, line)
}

/// Read a field off a caught `Error` value. A field other than `type`,
/// `message`, `file`, or `line` is a catchable [`ErrorKind::AttrError`].
pub fn error_field(err: &ErrorData, name: &str) -> DogeResult {
    match name {
        "type" => Ok(Value::str(err.kind.as_str())),
        "message" => Ok(Value::Str(err.message.clone())),
        "file" => Ok(Value::Str(err.file.clone())),
        "line" => Ok(Value::int(err.line)),
        _ => Err(DogeError::attr_error(format!(
            "an Error has no field {name}"
        ))),
    }
}

/// Enter one call: fail (catchably) if the chain is already [`RECURSION_LIMIT`]
/// deep, otherwise record the new depth. Pairs with [`exit_call`].
pub fn enter_call(depth: &mut usize) -> DogeResult<()> {
    if *depth >= RECURSION_LIMIT {
        return Err(DogeError::new(
            ErrorKind::RecursionLimit,
            "too much recursion — more than 1000 calls deep",
        ));
    }
    *depth += 1;
    Ok(())
}

/// Leave one call, undoing a matching [`enter_call`].
pub fn exit_call(depth: &mut usize) {
    *depth = depth.saturating_sub(1);
}

/// The result of any fallible runtime operation. Defaults to yielding a
/// [`crate::Value`] since that is what almost every operator and builtin
/// produces.
pub type DogeResult<T = crate::Value> = Result<T, DogeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonk_error_message_is_the_barked_form() {
        assert_eq!(bonk_error(&Value::int(5)).message, "5");
        assert_eq!(bonk_error(&Value::str("much fail")).message, "much fail");
        assert_eq!(bonk_error(&Value::int(5)).kind, ErrorKind::Bonk);
    }

    #[test]
    fn assert_error_uses_message_or_default() {
        let with_message = assert_error(Some(&Value::str("age much wrong")));
        assert_eq!(with_message.kind, ErrorKind::AssertError);
        assert_eq!(with_message.message, "age much wrong");

        let without = assert_error(None);
        assert_eq!(without.kind, ErrorKind::AssertError);
        assert_eq!(without.message, ASSERT_DEFAULT_MESSAGE);
    }

    #[test]
    fn re_bonking_an_error_preserves_type_and_location() {
        let caught = error_value(&DogeError::key_error("no such key"), "main.doge", 7);
        let re_raised = bonk_error(&caught);
        assert_eq!(re_raised.kind, ErrorKind::KeyError);
        assert_eq!(re_raised.message, "no such key");
        let loc = re_raised
            .location
            .expect("re-raised error keeps its location");
        assert_eq!(&*loc.file, "main.doge");
        assert_eq!(loc.line, 7);
    }

    #[test]
    fn error_value_carries_type_message_and_catch_site() {
        let err = DogeError::type_error("nope");
        match error_value(&err, "script.doge", 3) {
            Value::Error(e) => {
                assert_eq!(e.kind, ErrorKind::TypeError);
                assert_eq!(&*e.message, "nope");
                assert_eq!(&*e.file, "script.doge");
                assert_eq!(e.line, 3);
            }
            other => panic!("expected an Error, got {other:?}"),
        }
    }

    #[test]
    fn error_value_prefers_an_embedded_location_over_the_catch_site() {
        let raised = error_value(&DogeError::overflow("too big"), "raise.doge", 2);
        let re_raised = error_value(&bonk_error(&raised), "catch.doge", 99);
        match re_raised {
            Value::Error(e) => {
                assert_eq!(&*e.file, "raise.doge");
                assert_eq!(e.line, 2);
            }
            other => panic!("expected an Error, got {other:?}"),
        }
    }

    #[test]
    fn error_field_reads_the_four_fields_and_rejects_others() {
        let value = error_value(&DogeError::value_error("bad"), "f.doge", 4);
        let Value::Error(e) = value else {
            panic!("expected an Error");
        };
        assert!(matches!(error_field(&e, "type").unwrap(), Value::Str(s) if &*s == "ValueError"));
        assert!(matches!(error_field(&e, "message").unwrap(), Value::Str(s) if &*s == "bad"));
        assert!(matches!(error_field(&e, "file").unwrap(), Value::Str(s) if &*s == "f.doge"));
        assert!(crate::values_equal(
            &error_field(&e, "line").unwrap(),
            &Value::int(4)
        ));
        assert_eq!(
            error_field(&e, "nope").unwrap_err().kind,
            ErrorKind::AttrError
        );
    }

    #[test]
    fn enter_call_errors_past_limit() {
        let mut depth = 0;
        for _ in 0..RECURSION_LIMIT {
            enter_call(&mut depth).expect("within the limit");
        }
        let err = enter_call(&mut depth).expect_err("one past the limit");
        assert_eq!(err.kind, ErrorKind::RecursionLimit);
        assert_eq!(depth, RECURSION_LIMIT);
    }

    #[test]
    fn exit_call_saturates_at_zero() {
        let mut depth = 0;
        exit_call(&mut depth);
        assert_eq!(depth, 0);
    }
}
