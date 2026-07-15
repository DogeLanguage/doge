use super::*;
use crate::error::ErrorKind;
use crate::ordered_map::OrderedMap;
use bigdecimal::ToPrimitive;

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
    let xs = list(vec![Value::int(1)]);
    assert!(matches!(
        call(&xs, "append", vec![Value::int(2)]).unwrap(),
        Value::None
    ));
    assert!(crate::values_equal(
        &call(&xs, "pop", vec![]).unwrap(),
        &Value::int(2)
    ));
    assert!(crate::values_equal(
        &call(&xs, "pop", vec![]).unwrap(),
        &Value::int(1)
    ));
    assert_eq!(
        call(&xs, "pop", vec![]).unwrap_err().kind,
        ErrorKind::IndexOutOfBounds
    );
}

#[test]
fn insert_places_and_bounds_check() {
    let xs = list(vec![Value::int(1), Value::int(3)]);
    call(&xs, "insert", vec![Value::int(1), Value::int(2)]).unwrap();
    call(&xs, "insert", vec![Value::int(0), Value::int(0)]).unwrap();
    // insert at len appends; negative counts from the end.
    call(&xs, "insert", vec![Value::int(4), Value::int(4)]).unwrap();
    call(&xs, "insert", vec![Value::int(-1), Value::int(9)]).unwrap();
    if let Value::List(items) = &xs {
        let got: Vec<i64> = items
            .borrow()
            .iter()
            .map(|v| match v {
                Value::Int(n) => n.to_i64().unwrap(),
                _ => panic!("expected Int"),
            })
            .collect();
        assert_eq!(got, [0, 1, 2, 3, 9, 4]);
    }
    assert_eq!(
        call(&xs, "insert", vec![Value::int(99), Value::int(0)])
            .unwrap_err()
            .kind,
        ErrorKind::IndexOutOfBounds
    );
    assert_eq!(
        call(&xs, "insert", vec![Value::str("x"), Value::int(0)])
            .unwrap_err()
            .kind,
        ErrorKind::TypeError
    );
}

#[test]
fn remove_and_index_of_hit_and_miss() {
    let xs = list(vec![Value::int(1), Value::int(2), Value::int(2)]);
    assert!(crate::values_equal(
        &call(&xs, "index_of", vec![Value::int(2)]).unwrap(),
        &Value::int(1)
    ));
    call(&xs, "remove", vec![Value::int(2)]).unwrap();
    if let Value::List(items) = &xs {
        assert_eq!(items.borrow().len(), 2);
    }
    assert_eq!(
        call(&xs, "remove", vec![Value::int(7)]).unwrap_err().kind,
        ErrorKind::ValueError
    );
    assert_eq!(
        call(&xs, "index_of", vec![Value::int(7)]).unwrap_err().kind,
        ErrorKind::ValueError
    );
}

#[test]
fn sort_orders_numbers_and_strings_and_rejects_mixed() {
    let nums = list(vec![Value::int(3), Value::Float(1.5), Value::int(2)]);
    call(&nums, "sort", vec![]).unwrap();
    if let Value::List(items) = &nums {
        let items = items.borrow();
        assert!(matches!(items[0], Value::Float(f) if f == 1.5));
        assert!(crate::values_equal(&items[2], &Value::int(3)));
    }
    let mixed = list(vec![Value::int(1), Value::str("x")]);
    assert_eq!(
        call(&mixed, "sort", vec![]).unwrap_err().kind,
        ErrorKind::TypeError
    );
}

#[test]
fn reverse_contains_and_clear() {
    let xs = list(vec![Value::int(1), Value::int(2), Value::int(3)]);
    call(&xs, "reverse", vec![]).unwrap();
    if let Value::List(items) = &xs {
        assert!(crate::values_equal(&items.borrow()[0], &Value::int(3)));
    }
    assert!(matches!(
        call(&xs, "contains", vec![Value::int(2)]).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        call(&xs, "contains", vec![Value::int(9)]).unwrap(),
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
        ("age", Value::int(7)),
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
    let d = dict(&[("a", Value::int(1)), ("b", Value::int(2))]);
    assert!(matches!(
        call(&d, "has", vec![Value::str("a")]).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        call(&d, "has", vec![Value::str("z")]).unwrap(),
        Value::Bool(false)
    ));
    assert!(crate::values_equal(
        &call(&d, "remove", vec![Value::str("a")]).unwrap(),
        &Value::int(1)
    ));
    assert_eq!(
        call(&d, "remove", vec![Value::str("z")]).unwrap_err().kind,
        ErrorKind::KeyError
    );
    assert_eq!(
        call(&d, "has", vec![Value::int(1)]).unwrap_err().kind,
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
    let err = call(&Value::int(5), "append", vec![Value::int(1)]).unwrap_err();
    assert_eq!(err.kind, ErrorKind::AttrError);
    assert_eq!(err.message, "an Int has no methods");
}

#[test]
fn has_builtin_method_agrees_with_the_dispatch_tables() {
    // Every name the gate reports must actually dispatch — a real method fails on
    // arity or type, never with "has no method". This keeps `LIST_METHODS` /
    // `DICT_METHODS` in step with the `match` arms binding relies on.
    let xs = list(vec![]);
    for name in list::LIST_METHODS {
        assert!(has_builtin_method(&xs, name));
        if let Err(e) = call(&xs, name, vec![]) {
            assert_ne!(e.message, format!("a List has no method {name}"));
        }
    }
    let d = dict(&[]);
    for name in dict::DICT_METHODS {
        assert!(has_builtin_method(&d, name));
        if let Err(e) = call(&d, name, vec![]) {
            assert_ne!(e.message, format!("a Dict has no method {name}"));
        }
    }
    let s = Value::str("");
    for name in str::STR_METHODS {
        assert!(has_builtin_method(&s, name));
        if let Err(e) = call(&s, name, vec![]) {
            assert_ne!(e.message, format!("a Str has no method {name}"));
        }
    }
    // A name in neither table does not bind and is not a method.
    assert!(!has_builtin_method(&xs, "nope"));
    assert!(!has_builtin_method(&Value::int(1), "append"));
}
