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

/// Resolve a callee to the function it names. Calling anything that is not a
/// function is a catchable `TypeError`, worded from the caller's point of view.
pub fn callee_function(value: &Value) -> DogeResult<Rc<FunctionData>> {
    match value {
        Value::Function(f) => Ok(Rc::clone(f)),
        other => Err(DogeError::type_error(format!(
            "cannot call {} — it is not a function",
            other.describe()
        ))),
    }
}

/// The error an indirect call raises when the argument count is wrong, worded
/// like the compiler's user-function arity message.
pub fn function_arity_error(name: &str, expected: usize, got: usize) -> DogeError {
    let noun = if expected == 1 {
        "argument"
    } else {
        "arguments"
    };
    DogeError::type_error(format!("{name} takes {expected} {noun}, got {got}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn cell_round_trips_and_shares() {
        let cell: Cell = Rc::new(std::cell::RefCell::new(Value::Int(1)));
        assert!(matches!(cell_get(&cell), Value::Int(1)));
        let shared = Rc::clone(&cell);
        cell_set(&cell, Value::Int(2));
        // The write is visible through the shared handle.
        assert!(matches!(cell_get(&shared), Value::Int(2)));
    }

    #[test]
    fn callee_function_unwraps_a_function() {
        let f = Value::function(3, "greet", vec![]);
        let got = callee_function(&f).expect("a function");
        assert_eq!(got.fn_id, 3);
    }

    #[test]
    fn calling_a_non_function_is_a_catchable_type_error() {
        let err = callee_function(&Value::Int(1)).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
        assert_eq!(err.message, "cannot call an Int — it is not a function");
    }

    #[test]
    fn function_arity_error_matches_the_user_wording() {
        assert_eq!(
            function_arity_error("greet", 2, 1).message,
            "greet takes 2 arguments, got 1"
        );
        assert_eq!(
            function_arity_error("f", 1, 0).message,
            "f takes 1 argument, got 0"
        );
    }
}
