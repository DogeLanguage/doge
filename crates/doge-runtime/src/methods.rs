//! Built-in method dispatch for collection values — the counterpart of
//! [`crate::objects`] for `many` instances. A method call on a non-`Object`
//! receiver (`xs.append(1)`, `d.keys()`) routes here; the generated `call_method`
//! dispatcher handles `Object` receivers itself and forwards everything else.
//!
//! Behaviour lives here, not in codegen (generated Rust is thin glue): each
//! method mutates or reads the value's shared cell and returns a `Value`, and
//! every failure is a catchable [`DogeError`], never a panic.

use std::cmp::Ordering;

use crate::error::{DogeError, DogeResult};
use crate::objects::method_arity_error;
use crate::ops::{order, values_equal};
use crate::value::Value;

/// Dispatch a method call on a List or Dict. `recv` is the receiver, `name` the
/// method, `args` the already-evaluated arguments. A non-collection receiver, an
/// unknown method, or a wrong argument count is a catchable error.
pub fn builtin_method(recv: &Value, name: &str, args: Vec<Value>) -> DogeResult {
    match recv {
        Value::List(_) => list_method(recv, name, args),
        Value::Dict(_) => dict_method(recv, name, args),
        _ => Err(DogeError::type_error(format!(
            "cannot call {name} on {}",
            recv.describe()
        ))),
    }
}

/// The arity gate every method runs first: reuses the object-method wording so a
/// List/Dict arity error reads exactly like `Shibe.speak takes 1 argument, got 0`.
fn check_arity(class: &str, method: &str, expected: usize, got: usize) -> DogeResult<()> {
    if got == expected {
        Ok(())
    } else {
        Err(method_arity_error(class, method, expected, got))
    }
}

fn list_method(recv: &Value, name: &str, mut args: Vec<Value>) -> DogeResult {
    let Value::List(items) = recv else {
        unreachable!("compiler bug: list_method called on a non-List")
    };
    let argc = args.len();
    match name {
        "append" => {
            check_arity("List", name, 1, argc)?;
            items.borrow_mut().push(args.remove(0));
            Ok(Value::None)
        }
        "pop" => {
            check_arity("List", name, 0, argc)?;
            items
                .borrow_mut()
                .pop()
                .ok_or_else(|| DogeError::index_out_of_bounds("cannot pop from an empty List"))
        }
        "insert" => {
            check_arity("List", name, 2, argc)?;
            let index = args.remove(0);
            let item = args.remove(0);
            let Value::Int(i) = index else {
                return Err(DogeError::type_error(format!(
                    "List.insert needs an Int index, got {}",
                    index.describe()
                )));
            };
            let mut list = items.borrow_mut();
            let len = list.len() as i64;
            let idx = if i < 0 { i + len } else { i };
            if idx < 0 || idx > len {
                return Err(DogeError::index_out_of_bounds(format!(
                    "index {i} is out of bounds for length {len}"
                )));
            }
            list.insert(idx as usize, item);
            Ok(Value::None)
        }
        "remove" => {
            check_arity("List", name, 1, argc)?;
            let target = args.remove(0);
            let pos = items
                .borrow()
                .iter()
                .position(|element| values_equal(element, &target));
            match pos {
                Some(p) => {
                    items.borrow_mut().remove(p);
                    Ok(Value::None)
                }
                None => Err(DogeError::value_error("List.remove: item not found")),
            }
        }
        "index_of" => {
            check_arity("List", name, 1, argc)?;
            let target = args.remove(0);
            let pos = items
                .borrow()
                .iter()
                .position(|element| values_equal(element, &target));
            match pos {
                Some(p) => Ok(Value::Int(p as i64)),
                None => Err(DogeError::value_error("List.index_of: item not found")),
            }
        }
        "contains" => {
            check_arity("List", name, 1, argc)?;
            let target = args.remove(0);
            let found = items
                .borrow()
                .iter()
                .any(|element| values_equal(element, &target));
            Ok(Value::Bool(found))
        }
        "sort" => {
            check_arity("List", name, 0, argc)?;
            let mut list = items.borrow_mut();
            validate_sortable(&list)?;
            // `order` cannot fail here: validate_sortable guarantees a total
            // order, so the `Equal` fallback is never reached.
            list.sort_by(|a, b| order(a, b).unwrap_or(Ordering::Equal));
            Ok(Value::None)
        }
        "reverse" => {
            check_arity("List", name, 0, argc)?;
            items.borrow_mut().reverse();
            Ok(Value::None)
        }
        "clear" => {
            check_arity("List", name, 0, argc)?;
            items.borrow_mut().clear();
            Ok(Value::None)
        }
        _ => Err(DogeError::attr_error(format!(
            "a List has no method {name}"
        ))),
    }
}

/// Every element must be a Str, or every element a non-NaN number. An empty List
/// is trivially sortable. The check runs BEFORE sorting so the comparator always
/// sees a total order — a broken order could make std's sort panic, which the
/// runtime never does.
fn validate_sortable(items: &[Value]) -> DogeResult<()> {
    let all_str = items.iter().all(|v| matches!(v, Value::Str(_)));
    let all_num = items.iter().all(|v| match v {
        Value::Int(_) => true,
        Value::Float(f) => !f.is_nan(),
        _ => false,
    });
    if all_str || all_num {
        Ok(())
    } else {
        Err(DogeError::type_error(
            "List.sort needs all Ints/Floats or all Strs",
        ))
    }
}

fn dict_method(recv: &Value, name: &str, mut args: Vec<Value>) -> DogeResult {
    let Value::Dict(entries) = recv else {
        unreachable!("compiler bug: dict_method called on a non-Dict")
    };
    let argc = args.len();
    match name {
        "keys" => {
            check_arity("Dict", name, 0, argc)?;
            let keys = entries
                .borrow()
                .iter()
                .map(|(k, _)| Value::str(k))
                .collect();
            Ok(Value::list(keys))
        }
        "values" => {
            check_arity("Dict", name, 0, argc)?;
            let values = entries.borrow().iter().map(|(_, v)| v.clone()).collect();
            Ok(Value::list(values))
        }
        "items" => {
            check_arity("Dict", name, 0, argc)?;
            let items = entries
                .borrow()
                .iter()
                .map(|(k, v)| Value::list(vec![Value::str(k), v.clone()]))
                .collect();
            Ok(Value::list(items))
        }
        "has" => {
            check_arity("Dict", name, 1, argc)?;
            let key = args.remove(0);
            let Value::Str(k) = key else {
                return Err(DogeError::type_error(format!(
                    "Dict.has needs a Str key, got {}",
                    key.describe()
                )));
            };
            Ok(Value::Bool(entries.borrow().contains_key(k.as_ref())))
        }
        "remove" => {
            check_arity("Dict", name, 1, argc)?;
            let key = args.remove(0);
            let Value::Str(k) = key else {
                return Err(DogeError::type_error(format!(
                    "Dict.remove needs a Str key, got {}",
                    key.describe()
                )));
            };
            entries
                .borrow_mut()
                .remove(k.as_ref())
                .ok_or_else(|| DogeError::key_error(format!("no such key: {k:?}")))
        }
        "clear" => {
            check_arity("Dict", name, 0, argc)?;
            entries.borrow_mut().clear();
            Ok(Value::None)
        }
        _ => Err(DogeError::attr_error(format!(
            "a Dict has no method {name}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::ordered_map::OrderedMap;

    fn list(items: Vec<Value>) -> Value {
        Value::list(items)
    }

    fn dict(pairs: &[(&str, Value)]) -> Value {
        let mut m = OrderedMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        Value::dict(m)
    }

    fn call(recv: &Value, name: &str, args: Vec<Value>) -> DogeResult {
        builtin_method(recv, name, args)
    }

    #[test]
    fn append_and_pop_mutate_in_place() {
        let xs = list(vec![Value::Int(1)]);
        assert!(matches!(
            call(&xs, "append", vec![Value::Int(2)]).unwrap(),
            Value::None
        ));
        assert!(matches!(call(&xs, "pop", vec![]).unwrap(), Value::Int(2)));
        assert!(matches!(call(&xs, "pop", vec![]).unwrap(), Value::Int(1)));
        assert_eq!(
            call(&xs, "pop", vec![]).unwrap_err().kind,
            ErrorKind::IndexOutOfBounds
        );
    }

    #[test]
    fn insert_places_and_bounds_check() {
        let xs = list(vec![Value::Int(1), Value::Int(3)]);
        call(&xs, "insert", vec![Value::Int(1), Value::Int(2)]).unwrap();
        call(&xs, "insert", vec![Value::Int(0), Value::Int(0)]).unwrap();
        // insert at len appends; negative counts from the end.
        call(&xs, "insert", vec![Value::Int(4), Value::Int(4)]).unwrap();
        call(&xs, "insert", vec![Value::Int(-1), Value::Int(9)]).unwrap();
        if let Value::List(items) = &xs {
            let got: Vec<i64> = items
                .borrow()
                .iter()
                .map(|v| match v {
                    Value::Int(n) => *n,
                    _ => panic!("expected Int"),
                })
                .collect();
            assert_eq!(got, [0, 1, 2, 3, 9, 4]);
        }
        assert_eq!(
            call(&xs, "insert", vec![Value::Int(99), Value::Int(0)])
                .unwrap_err()
                .kind,
            ErrorKind::IndexOutOfBounds
        );
        assert_eq!(
            call(&xs, "insert", vec![Value::str("x"), Value::Int(0)])
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn remove_and_index_of_hit_and_miss() {
        let xs = list(vec![Value::Int(1), Value::Int(2), Value::Int(2)]);
        assert!(matches!(
            call(&xs, "index_of", vec![Value::Int(2)]).unwrap(),
            Value::Int(1)
        ));
        call(&xs, "remove", vec![Value::Int(2)]).unwrap();
        if let Value::List(items) = &xs {
            assert_eq!(items.borrow().len(), 2);
        }
        assert_eq!(
            call(&xs, "remove", vec![Value::Int(7)]).unwrap_err().kind,
            ErrorKind::ValueError
        );
        assert_eq!(
            call(&xs, "index_of", vec![Value::Int(7)]).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn sort_orders_numbers_and_strings_and_rejects_mixed() {
        let nums = list(vec![Value::Int(3), Value::Float(1.5), Value::Int(2)]);
        call(&nums, "sort", vec![]).unwrap();
        if let Value::List(items) = &nums {
            let items = items.borrow();
            assert!(matches!(items[0], Value::Float(f) if f == 1.5));
            assert!(matches!(items[2], Value::Int(3)));
        }
        let mixed = list(vec![Value::Int(1), Value::str("x")]);
        assert_eq!(
            call(&mixed, "sort", vec![]).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn reverse_contains_and_clear() {
        let xs = list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        call(&xs, "reverse", vec![]).unwrap();
        if let Value::List(items) = &xs {
            assert!(matches!(items.borrow()[0], Value::Int(3)));
        }
        assert!(matches!(
            call(&xs, "contains", vec![Value::Int(2)]).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            call(&xs, "contains", vec![Value::Int(9)]).unwrap(),
            Value::Bool(false)
        ));
        call(&xs, "clear", vec![]).unwrap();
        if let Value::List(items) = &xs {
            assert!(items.borrow().is_empty());
        }
    }

    #[test]
    fn dict_keys_values_items_in_insertion_order() {
        let d = dict(&[
            ("name", Value::str("kabosu")),
            ("kind", Value::str("shibe")),
            ("age", Value::Int(7)),
        ]);
        let keys = call(&d, "keys", vec![]).unwrap();
        if let Value::List(items) = &keys {
            let got: Vec<String> = items.borrow().iter().map(|v| v.to_string()).collect();
            assert_eq!(got, ["name", "kind", "age"]);
        } else {
            panic!("expected a List");
        }
        let values = call(&d, "values", vec![]).unwrap();
        assert!(matches!(values, Value::List(_)));
        let items = call(&d, "items", vec![]).unwrap();
        if let Value::List(pairs) = &items {
            let first = pairs.borrow()[0].clone();
            assert!(matches!(first, Value::List(_)));
        } else {
            panic!("expected a List of pairs");
        }
    }

    #[test]
    fn dict_has_remove_and_clear() {
        let d = dict(&[("a", Value::Int(1)), ("b", Value::Int(2))]);
        assert!(matches!(
            call(&d, "has", vec![Value::str("a")]).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            call(&d, "has", vec![Value::str("z")]).unwrap(),
            Value::Bool(false)
        ));
        assert!(matches!(
            call(&d, "remove", vec![Value::str("a")]).unwrap(),
            Value::Int(1)
        ));
        assert_eq!(
            call(&d, "remove", vec![Value::str("z")]).unwrap_err().kind,
            ErrorKind::KeyError
        );
        assert_eq!(
            call(&d, "has", vec![Value::Int(1)]).unwrap_err().kind,
            ErrorKind::TypeError
        );
        call(&d, "clear", vec![]).unwrap();
        if let Value::Dict(entries) = &d {
            assert!(entries.borrow().is_empty());
        }
    }

    #[test]
    fn arity_unknown_method_and_non_collection() {
        let xs = list(vec![]);
        assert_eq!(
            call(&xs, "append", vec![]).unwrap_err().message,
            "List.append takes 1 argument, got 0"
        );
        let err = call(&xs, "flop", vec![]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::AttrError);
        assert_eq!(err.message, "a List has no method flop");
        let err = call(&Value::Int(5), "append", vec![Value::Int(1)]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
        assert_eq!(err.message, "cannot call append on an Int");
    }
}
