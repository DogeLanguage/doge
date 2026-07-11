use super::*;
use crate::error::ErrorKind;

fn int(n: i64) -> Value {
    Value::Int(n)
}
fn float(f: f64) -> Value {
    Value::Float(f)
}

#[test]
fn division_always_float() {
    // 5 / 2 is 2.5, never 2 — the whole reason `/` differs from `//`.
    assert!(matches!(div(int(5), int(2)).unwrap(), Value::Float(f) if f == 2.5));
    assert!(matches!(div(int(4), int(2)).unwrap(), Value::Float(f) if f == 2.0));
}

#[test]
fn floordiv_is_integer_division() {
    assert!(matches!(floordiv(int(5), int(2)).unwrap(), Value::Int(2)));
    // Floors toward negative infinity, Python-style, not toward zero.
    assert!(matches!(floordiv(int(-7), int(2)).unwrap(), Value::Int(-4)));
    assert!(matches!(floordiv(int(7), int(-2)).unwrap(), Value::Int(-4)));
}

#[test]
fn floordiv_and_rem_are_consistent() {
    // a == (a // b) * b + (a % b) for the tricky negative cases.
    for (a, b) in [(-7, 2), (7, -2), (-7, -2), (7, 2)] {
        let q = match floordiv(int(a), int(b)).unwrap() {
            Value::Int(n) => n,
            _ => unreachable!(),
        };
        let r = match rem(int(a), int(b)).unwrap() {
            Value::Int(n) => n,
            _ => unreachable!(),
        };
        assert_eq!(a, q * b + r, "identity failed for {a} // {b}");
    }
}

#[test]
fn mixed_int_float_promotion() {
    assert!(matches!(add(int(1), float(0.5)).unwrap(), Value::Float(f) if f == 1.5));
    assert!(matches!(mul(float(2.0), int(3)).unwrap(), Value::Float(f) if f == 6.0));
}

#[test]
fn overflow_is_catchable_error() {
    // i64::MAX + 1 is an error a program can catch, never a silent wrap.
    let err = add(int(i64::MAX), int(1)).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Overflow);
}

#[test]
fn division_by_zero_is_catchable() {
    assert_eq!(
        div(int(1), int(0)).unwrap_err().kind,
        ErrorKind::DivisionByZero
    );
    assert_eq!(
        floordiv(int(1), int(0)).unwrap_err().kind,
        ErrorKind::DivisionByZero
    );
    assert_eq!(
        rem(int(1), int(0)).unwrap_err().kind,
        ErrorKind::DivisionByZero
    );
}

#[test]
fn string_and_list_concatenation() {
    assert!(
        matches!(add(Value::str("much "), Value::str("wow")).unwrap(), Value::Str(s) if &*s == "much wow")
    );
    let joined = add(Value::list(vec![int(1)]), Value::list(vec![int(2), int(3)])).unwrap();
    match joined {
        Value::List(items) => assert_eq!(items.borrow().len(), 3),
        _ => panic!("expected a list"),
    }
}

#[test]
fn wrong_type_operands_are_type_errors() {
    let err = add(Value::str("dog"), int(5)).unwrap_err();
    assert_eq!(err.kind, ErrorKind::TypeError);
    assert_eq!(err.message, "cannot + a Str and an Int");
}

#[test]
fn equality_cross_numeric() {
    assert!(values_equal(&int(1), &float(1.0)));
    assert!(!values_equal(&int(1), &float(1.5)));
    // Different types are simply unequal, never an error.
    assert!(!values_equal(&int(1), &Value::str("1")));
}

#[test]
fn objects_are_equal_only_by_identity() {
    let a = Value::object(0, "Shibe");
    // A clone shares the same Rc — still the same object.
    assert!(values_equal(&a, &a.clone()));
    // A second, independently built instance is a different object.
    let b = Value::object(0, "Shibe");
    assert!(!values_equal(&a, &b));
}

#[test]
fn ordering_across_numbers_and_strings() {
    assert!(matches!(lt(int(1), float(1.5)).unwrap(), Value::Bool(true)));
    assert!(matches!(
        gt(Value::str("cheems"), Value::str("bonk")).unwrap(),
        Value::Bool(true)
    ));
    // Comparing across incomparable types is a catchable error.
    assert_eq!(
        lt(int(1), Value::str("x")).unwrap_err().kind,
        ErrorKind::TypeError
    );
}

#[test]
fn string_indexing_is_char_based() {
    // Byte indexing would split the two-byte 'é'; Doge indexes characters.
    let hello = Value::str("héllo");
    assert!(matches!(index_get(&hello, &int(1)).unwrap(), Value::Str(s) if &*s == "é"));
    assert!(matches!(index_get(&hello, &int(0)).unwrap(), Value::Str(s) if &*s == "h"));
}

#[test]
fn negative_indices_count_from_the_end() {
    let xs = Value::list(vec![int(10), int(20), int(30)]);
    assert!(matches!(index_get(&xs, &int(-1)).unwrap(), Value::Int(30)));
    assert!(matches!(index_get(&xs, &int(-3)).unwrap(), Value::Int(10)));
}

#[test]
fn oob_index_is_catchable_error() {
    let xs = Value::list(vec![int(1), int(2)]);
    assert_eq!(
        index_get(&xs, &int(5)).unwrap_err().kind,
        ErrorKind::IndexOutOfBounds
    );
    assert_eq!(
        index_get(&xs, &int(-3)).unwrap_err().kind,
        ErrorKind::IndexOutOfBounds
    );
}

#[test]
fn missing_dict_key_is_catchable_error() {
    let mut map = crate::ordered_map::OrderedMap::new();
    map.insert("name".to_string(), Value::str("kabosu"));
    let d = Value::dict(map);
    assert!(
        matches!(index_get(&d, &Value::str("name")).unwrap(), Value::Str(s) if &*s == "kabosu")
    );
    assert_eq!(
        index_get(&d, &Value::str("age")).unwrap_err().kind,
        ErrorKind::KeyError
    );
}

#[test]
fn index_set_mutates_list_and_dict() {
    let xs = Value::list(vec![int(1), int(2)]);
    index_set(&xs, &int(0), int(99)).unwrap();
    assert!(matches!(index_get(&xs, &int(0)).unwrap(), Value::Int(99)));

    let d = Value::dict(crate::ordered_map::OrderedMap::new());
    index_set(&d, &Value::str("k"), int(7)).unwrap();
    assert!(matches!(
        index_get(&d, &Value::str("k")).unwrap(),
        Value::Int(7)
    ));

    // Strings are immutable — a catchable type error, not a panic.
    assert_eq!(
        index_set(&Value::str("dog"), &int(0), Value::str("x"))
            .unwrap_err()
            .kind,
        ErrorKind::TypeError
    );
}

#[test]
fn negation_and_not() {
    assert!(matches!(neg(int(5)).unwrap(), Value::Int(-5)));
    assert!(matches!(neg(float(2.5)).unwrap(), Value::Float(f) if f == -2.5));
    assert_eq!(neg(int(i64::MIN)).unwrap_err().kind, ErrorKind::Overflow);
    assert!(matches!(not_(int(0)).unwrap(), Value::Bool(true)));
    assert!(matches!(
        not_(Value::str("dog")).unwrap(),
        Value::Bool(false)
    ));
}

#[test]
fn iter_value_snapshots_a_list() {
    let xs = Value::list(vec![int(1), int(2)]);
    let snapshot = iter_value(&xs).unwrap();
    // Pushing to the original after the snapshot must not grow the snapshot.
    if let Value::List(items) = &xs {
        items.borrow_mut().push(int(3));
    }
    assert_eq!(snapshot.len(), 2);
}

#[test]
fn iter_value_walks_str_characters() {
    // Char-based, not byte-based — the two-byte 'é' is a single element.
    let chars = iter_value(&Value::str("héllo")).unwrap();
    assert_eq!(chars.len(), 5);
    assert!(matches!(&chars[1], Value::Str(s) if &**s == "é"));
}

#[test]
fn iter_value_rejects_int() {
    assert_eq!(iter_value(&int(7)).unwrap_err().kind, ErrorKind::TypeError);
}
