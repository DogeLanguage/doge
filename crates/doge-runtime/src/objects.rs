//! Field access and method-dispatch helpers for `many Name:` instances. The
//! generated code calls these; the object model — fields on assignment, missing
//! field/method as a catchable error — lives here, not in codegen (Hard Rule 5).

use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// A class name with its English article, e.g. `"a Shibe"` / `"an Ostrich"`.
/// Objects always describe themselves by class, never as "an Object".
fn a_class(name: &str) -> String {
    let article = match name.chars().next() {
        Some('A' | 'E' | 'I' | 'O' | 'U' | 'a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    };
    format!("{article} {name}")
}

/// Read `obj.name`. A missing field is a catchable [`ErrorKind::AttrError`];
/// reading a field off a non-object is a catchable `TypeError`.
///
/// [`ErrorKind::AttrError`]: crate::ErrorKind::AttrError
pub fn attr_get(obj: &Value, name: &str) -> DogeResult {
    match obj {
        Value::Object(o) => {
            let data = o.borrow();
            data.fields.get(name).cloned().ok_or_else(|| {
                DogeError::attr_error(format!("{} has no field {name}", a_class(&data.class_name)))
            })
        }
        Value::Error(e) => crate::error::error_field(e, name),
        _ => Err(DogeError::type_error(format!(
            "cannot read the field {name} of {}",
            obj.describe()
        ))),
    }
}

/// Write `obj.name = value`. A field appears the first time it is assigned;
/// setting a field on a non-object is a catchable `TypeError`.
pub fn attr_set(obj: &Value, name: &str, value: Value) -> DogeResult<()> {
    match obj {
        Value::Object(o) => {
            o.borrow_mut().fields.insert(name.to_string(), value);
            Ok(())
        }
        _ => Err(DogeError::type_error(format!(
            "cannot set the field {name} on {}",
            obj.describe()
        ))),
    }
}

/// The class id of a method-call receiver, so the dispatcher can pick the right
/// arm. Calling a method on a value that has no methods is a catchable
/// `AttrError`.
pub fn object_class_id(recv: &Value) -> DogeResult<u32> {
    match recv {
        Value::Object(o) => Ok(o.borrow().class_id),
        _ => Err(no_methods_error(recv)),
    }
}

/// The error a method-call site raises when the receiver's class has no such
/// method. `recv` is an object at every real call site; the non-object branch is
/// a defensive fallback that mirrors [`object_class_id`].
pub fn no_such_method(recv: &Value, method: &str) -> DogeError {
    match recv {
        Value::Object(o) => {
            let data = o.borrow();
            DogeError::attr_error(format!(
                "{} has no method {method}",
                a_class(&data.class_name)
            ))
        }
        _ => no_methods_error(recv),
    }
}

/// The error raised when a method is called on a value whose type has no methods
/// at all (an Int, Str, Bool, …). Single source of the wording so it reads the
/// same whether the receiver reached here through the dispatcher or a builtin.
pub fn no_methods_error(recv: &Value) -> DogeError {
    DogeError::attr_error(format!("{} has no methods", recv.describe()))
}

/// The error a method call raises when the argument count is wrong, worded like
/// the compiler's user-function arity message. `max` is `None` when the method is
/// variadic.
pub fn method_arity_error(
    class: &str,
    method: &str,
    min: usize,
    max: Option<usize>,
    got: usize,
) -> DogeError {
    DogeError::type_error(crate::functions::arity_phrase(
        &format!("{class}.{method}"),
        min,
        max,
        got,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn set_then_get_round_trips_a_field() {
        let obj = Value::object(0, "Shibe");
        attr_set(&obj, "name", Value::str("kabosu")).unwrap();
        assert!(matches!(attr_get(&obj, "name").unwrap(), Value::Str(s) if &*s == "kabosu"));
    }

    #[test]
    fn missing_field_is_a_catchable_attr_error() {
        let obj = Value::object(0, "Shibe");
        let err = attr_get(&obj, "tail").unwrap_err();
        assert_eq!(err.kind, ErrorKind::AttrError);
        assert_eq!(err.message, "a Shibe has no field tail");
    }

    #[test]
    fn attr_on_a_non_object_is_a_type_error() {
        let err = attr_get(&Value::Int(1), "name").unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
        let err = attr_set(&Value::Int(1), "name", Value::None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
    }

    #[test]
    fn class_id_reads_the_object_and_rejects_others() {
        assert_eq!(object_class_id(&Value::object(3, "Shibe")).unwrap(), 3);
        let err = object_class_id(&Value::Int(1)).unwrap_err();
        assert_eq!(err.kind, ErrorKind::AttrError);
    }

    #[test]
    fn no_such_method_names_the_class() {
        let err = no_such_method(&Value::object(0, "Shibe"), "fly");
        assert_eq!(err.kind, ErrorKind::AttrError);
        assert_eq!(err.message, "a Shibe has no method fly");
    }

    #[test]
    fn no_methods_error_names_the_type_with_its_article() {
        let err = no_methods_error(&Value::Int(1));
        assert_eq!(err.kind, ErrorKind::AttrError);
        assert_eq!(err.message, "an Int has no methods");
        assert_eq!(
            no_methods_error(&Value::str("x")).message,
            "a Str has no methods"
        );
    }

    #[test]
    fn method_arity_error_matches_the_user_wording() {
        assert_eq!(
            method_arity_error("Shibe", "init", 2, Some(2), 1).message,
            "Shibe.init takes 2 arguments, got 1"
        );
        assert_eq!(
            method_arity_error("Shibe", "speak", 1, Some(1), 0).message,
            "Shibe.speak takes 1 argument, got 0"
        );
        assert_eq!(
            method_arity_error("Shibe", "greet", 1, None, 3).message,
            "Shibe.greet takes at least 1 argument, got 3"
        );
    }
}
