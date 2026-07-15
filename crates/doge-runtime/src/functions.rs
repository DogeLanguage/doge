use std::rc::Rc;

use crate::error::{DogeError, DogeResult};
use crate::value::{Cell, FunctionData, Value};

/// Read a captured cell, cloning the value out so the borrow ends here.
pub fn cell_get(cell: &Cell) -> Value {
    cell.borrow().clone()
}

/// Write a captured cell. Every writer shares the same cell, so the update is
/// visible to every closure that captured it.
pub fn cell_set(cell: &Cell, value: Value) {
    *cell.borrow_mut() = value;
}

/// Resolve a callee to the function it names. A class value calls the same way —
/// its `fn_id` is a constructor arm — so both unwrap to their shared
/// [`FunctionData`]. Calling anything else is a catchable `TypeError`, worded
/// from the caller's point of view.
pub fn callee_function(value: &Value) -> DogeResult<Rc<FunctionData>> {
    match value {
        Value::Function(f) | Value::Class(f) => Ok(Rc::clone(f)),
        other => Err(DogeError::type_error(format!(
            "cannot call {} — it is not a function",
            other.describe()
        ))),
    }
}

/// The "takes … arguments, got N" phrase shared by the function- and
/// method-arity errors. `max` is `None` for a variadic header (no upper bound);
/// when it equals `min` the header is fixed-arity and reads as a single count.
pub(crate) fn arity_phrase(subject: &str, min: usize, max: Option<usize>, got: usize) -> String {
    let noun = |count: usize| if count == 1 { "argument" } else { "arguments" };
    match max {
        Some(max) if max == min => {
            format!("{subject} takes {min} {}, got {got}", noun(min))
        }
        Some(max) => format!("{subject} takes {min} to {max} arguments, got {got}"),
        None => format!("{subject} takes at least {min} {}, got {got}", noun(min)),
    }
}

/// The error an indirect call raises when the argument count is wrong, worded
/// like the compiler's user-function arity message. `max` is `None` when the
/// function is variadic.
pub fn function_arity_error(name: &str, min: usize, max: Option<usize>, got: usize) -> DogeError {
    DogeError::type_error(arity_phrase(name, min, max, got))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn cell_round_trips_and_shares() {
        let cell: Cell = Rc::new(std::cell::RefCell::new(Value::int(1)));
        assert!(crate::values_equal(&cell_get(&cell), &Value::int(1)));
        let shared = Rc::clone(&cell);
        cell_set(&cell, Value::int(2));
        // The write is visible through the shared handle.
        assert!(crate::values_equal(&cell_get(&shared), &Value::int(2)));
    }

    #[test]
    fn callee_function_unwraps_a_function() {
        let f = Value::function(3, "greet", vec![]);
        let got = callee_function(&f).expect("a function");
        assert_eq!(got.fn_id, 3);
    }

    #[test]
    fn calling_a_non_function_is_a_catchable_type_error() {
        let err = callee_function(&Value::int(1)).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
        assert_eq!(err.message, "cannot call an Int — it is not a function");
    }

    #[test]
    fn function_arity_error_matches_the_user_wording() {
        assert_eq!(
            function_arity_error("greet", 2, Some(2), 1).message,
            "greet takes 2 arguments, got 1"
        );
        assert_eq!(
            function_arity_error("f", 1, Some(1), 0).message,
            "f takes 1 argument, got 0"
        );
    }

    #[test]
    fn function_arity_error_reports_ranges_and_variadics() {
        assert_eq!(
            function_arity_error("greet", 1, Some(3), 4).message,
            "greet takes 1 to 3 arguments, got 4"
        );
        assert_eq!(
            function_arity_error("party", 1, None, 0).message,
            "party takes at least 1 argument, got 0"
        );
    }
}
