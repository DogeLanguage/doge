use std::fmt;

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

/// The result of any fallible runtime operation. Defaults to yielding a
/// [`crate::Value`] since that is what almost every operator and builtin
/// produces.
pub type DogeResult<T = crate::Value> = Result<T, DogeError>;
