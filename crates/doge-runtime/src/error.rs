use std::fmt;

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
    /// Integer arithmetic that would exceed the `i64` range.
    Overflow,
    /// List/Str index outside the valid range.
    IndexOutOfBounds,
    /// Dict lookup for a key that is not present.
    KeyError,
    /// A value was the right type but not a usable value (e.g. `int("dog")`).
    ValueError,
    /// A `bonk` raised by the program itself.
    Bonk,
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
            ErrorKind::Bonk => "Bonk",
            ErrorKind::RecursionLimit => "RecursionLimit",
        }
    }
}

/// A catchable runtime error: a category plus a precise, plain-English message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DogeError {
    pub kind: ErrorKind,
    pub message: String,
}

impl DogeError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        DogeError {
            kind,
            message: message.into(),
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
}

impl fmt::Display for DogeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DogeError {}

/// Build the error a `bonk <expr>` raises: the message is the value's display
/// form, so `bonk 5` reads `5` and `bonk "much fail"` reads `much fail` — the
/// same text `bark` would print.
pub fn bonk_error(value: &Value) -> DogeError {
    DogeError::new(ErrorKind::Bonk, value.to_string())
}

/// The value bound by `oh no err!`: a `Str` carrying the caught error's message.
pub fn error_value(err: &DogeError) -> Value {
    Value::str(&err.message)
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
        assert_eq!(bonk_error(&Value::Int(5)).message, "5");
        assert_eq!(bonk_error(&Value::str("much fail")).message, "much fail");
        assert_eq!(bonk_error(&Value::Int(5)).kind, ErrorKind::Bonk);
    }

    #[test]
    fn error_value_carries_the_message() {
        let err = DogeError::type_error("nope");
        match error_value(&err) {
            Value::Str(s) => assert_eq!(&*s, "nope"),
            other => panic!("expected a Str, got {other:?}"),
        }
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
