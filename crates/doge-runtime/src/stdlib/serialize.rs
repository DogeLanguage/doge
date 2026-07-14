//! Pieces shared by the `json` and `dson` codecs: the recursion cap that keeps a
//! deeply nested (or cyclic) value from overflowing the stack, the type error for
//! a value that has no serialized form, and the string-escape writer both emitters
//! use. Everything the two formats do *differently* — number base, delimiters,
//! `\u` width — lives in the codec modules; only the genuinely identical logic is
//! here.

use crate::error::DogeError;
use crate::value::Value;

/// The deepest nesting either codec will parse or emit before giving up with a
/// catchable error. A recursive-descent parser on `[[[[…` and an emit of a value
/// that refers to itself would both otherwise grow the call stack without bound;
/// this bounds them so the failure is a `pls`/`oh no`-catchable error, never an
/// abort (Hard Rule 2).
pub(crate) const MAX_DEPTH: usize = 128;

/// The catchable error a codec raises when asked to serialize a value that has no
/// place in a data document (an object, function, socket, …). `module` names the
/// codec so the message reads `json.emit cannot serialize a Function`.
pub(crate) fn unsupported(module: &str, v: &Value) -> DogeError {
    DogeError::type_error(format!("{module}.emit cannot serialize {}", v.describe()))
}

/// The catchable error a codec raises when nesting passes [`MAX_DEPTH`].
pub(crate) fn too_deep(module: &str, action: &str) -> DogeError {
    DogeError::value_error(format!(
        "{module}.{action}: much deep. nesting past {MAX_DEPTH} levels"
    ))
}

/// Append `s` as a quoted, escaped string to `out`. The simple escapes (`\"`,
/// `\\`, `\n`, `\r`, `\t`, `\b`, `\f`) are identical across both formats; a control
/// character with no short escape is written by `unicode`, which is the one part
/// that differs (JSON uses four hex digits, DSON six octal ones).
pub(crate) fn escape_str(s: &str, out: &mut String, unicode: impl Fn(&mut String, u32)) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => unicode(out, c as u32),
            c => out.push(c),
        }
    }
    out.push('"');
}
