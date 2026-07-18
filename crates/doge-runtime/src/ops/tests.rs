use std::str::FromStr;

use bigdecimal::BigDecimal;
use num_bigint::BigInt;

use super::*;
use crate::error::{DogeError, ErrorKind};

fn int(n: i64) -> Value {
    Value::int(n)
}
fn float(f: f64) -> Value {
    Value::Float(f)
}
fn dec(s: &str) -> Value {
    Value::decimal(BigDecimal::from_str(s).unwrap())
}

#[test]
fn division_always_float() {
    // 5 / 2 is 2.5, never 2 — the whole reason `/` differs from `//`.
    assert!(matches!(div(int(5), int(2)).unwrap(), Value::Float(f) if f == 2.5));
    assert!(matches!(div(int(4), int(2)).unwrap(), Value::Float(f) if f == 2.0));
}

#[test]
fn floordiv_is_integer_division() {
    assert!(values_equal(&floordiv(int(5), int(2)).unwrap(), &int(2)));
    // Floors toward negative infinity, Python-style, not toward zero.
    assert!(values_equal(&floordiv(int(-7), int(2)).unwrap(), &int(-4)));
    assert!(values_equal(&floordiv(int(7), int(-2)).unwrap(), &int(-4)));
}

#[test]
fn floordiv_and_rem_are_consistent() {
    // a == (a // b) * b + (a % b) for the tricky negative cases.
    for (a, b) in [(-7, 2), (7, -2), (-7, -2), (7, 2)] {
        let q = floordiv(int(a), int(b)).unwrap();
        let r = rem(int(a), int(b)).unwrap();
        let rhs = add(mul(q, int(b)).unwrap(), r).unwrap();
        assert!(
            values_equal(&int(a), &rhs),
            "identity failed for {a} // {b}"
        );
    }
}

#[test]
fn mixed_int_float_promotion() {
    assert!(matches!(add(int(1), float(0.5)).unwrap(), Value::Float(f) if f == 1.5));
    assert!(matches!(mul(float(2.0), int(3)).unwrap(), Value::Float(f) if f == 6.0));
}

#[test]
fn strings_and_lists_repeat_in_both_operand_orders() {
    assert!(matches!(mul(Value::str("ab"), int(3)).unwrap(), Value::Str(s) if &*s == "ababab"));
    assert!(matches!(mul(int(2), Value::str("wow")).unwrap(), Value::Str(s) if &*s == "wowwow"));

    for repeated in [
        mul(Value::list(vec![int(1), int(2)]), int(2)).unwrap(),
        mul(int(2), Value::list(vec![int(1), int(2)])).unwrap(),
    ] {
        let Value::List(items) = repeated else {
            panic!("expected a list");
        };
        let items = items.borrow();
        assert_eq!(items.len(), 4);
        assert!(values_equal(&items[0], &int(1)));
        assert!(values_equal(&items[1], &int(2)));
        assert!(values_equal(&items[2], &int(1)));
        assert!(values_equal(&items[3], &int(2)));
    }
}

#[test]
fn non_positive_sequence_repetition_is_empty() {
    assert!(matches!(mul(Value::str("doge"), int(0)).unwrap(), Value::Str(s) if s.is_empty()));
    assert!(matches!(mul(int(-2), Value::str("doge")).unwrap(), Value::Str(s) if s.is_empty()));
    let huge_negative = Value::int(-(BigInt::from(usize::MAX) + 1u8));
    assert!(
        matches!(mul(Value::str("doge"), huge_negative).unwrap(), Value::Str(s) if s.is_empty())
    );

    for repeated in [
        mul(Value::list(vec![int(1)]), int(0)).unwrap(),
        mul(int(-2), Value::list(vec![int(1)])).unwrap(),
    ] {
        assert!(matches!(repeated, Value::List(items) if items.borrow().is_empty()));
    }
}

#[test]
fn repeated_lists_keep_shallow_element_identity() {
    let nested = Value::list(vec![int(1)]);
    let repeated = mul(Value::list(vec![nested]), int(2)).unwrap();
    let Value::List(items) = repeated else {
        panic!("expected a list");
    };
    let items = items.borrow();
    let (Value::List(first), Value::List(second)) = (&items[0], &items[1]) else {
        panic!("expected nested lists");
    };
    assert!(std::rc::Rc::ptr_eq(first, second));
}

#[test]
fn oversized_sequence_repetition_is_catchable() {
    let count = Value::int(BigInt::from(usize::MAX) + 1u8);
    assert_eq!(
        mul(Value::str("doge"), count).unwrap_err().kind,
        ErrorKind::Overflow
    );

    let count = Value::int(isize::MAX);
    assert_eq!(
        mul(Value::list(vec![int(1), int(2)]), count)
            .unwrap_err()
            .kind,
        ErrorKind::Overflow
    );
}

#[test]
fn int_grows_past_i64_instead_of_overflowing() {
    // i64::MAX + 1 no longer overflows — Int is arbitrary precision, so it simply
    // keeps more digits. Never a silent wrap, and never an error.
    let expected = Value::int(BigInt::from(i64::MAX) + 1);
    assert!(values_equal(
        &add(int(i64::MAX), int(1)).unwrap(),
        &expected
    ));
    // A product far past the range is exact, too.
    let big = mul(int(i64::MAX), int(i64::MAX)).unwrap();
    let expected = Value::int(BigInt::from(i64::MAX) * BigInt::from(i64::MAX));
    assert!(values_equal(&big, &expected));
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
fn decimals_are_exact_and_promote_with_ints() {
    // The canonical binary-float failure `0.1 + 0.2 != 0.3` holds exactly here.
    assert!(values_equal(
        &add(dec("0.1"), dec("0.2")).unwrap(),
        &dec("0.3")
    ));
    assert!(values_equal(
        &sub(dec("1.00"), dec("0.25")).unwrap(),
        &dec("0.75")
    ));
    assert!(values_equal(
        &mul(dec("1.5"), dec("2")).unwrap(),
        &dec("3.0")
    ));
    assert!(values_equal(
        &div(dec("1"), dec("4")).unwrap(),
        &dec("0.25")
    ));
    // Int and Decimal are both exact, so they promote to Decimal.
    assert!(values_equal(&add(int(1), dec("0.5")).unwrap(), &dec("1.5")));
    assert!(values_equal(&mul(dec("0.1"), int(3)).unwrap(), &dec("0.3")));
    // Trailing zeros don't affect equality — comparison is by value.
    assert!(values_equal(&dec("0.10"), &dec("0.1")));
}

#[test]
fn mixing_float_and_decimal_is_a_type_error() {
    // Silently joining an exact Decimal with an inexact Float would corrupt it.
    assert_eq!(
        add(dec("0.1"), float(0.2)).unwrap_err().kind,
        ErrorKind::TypeError
    );
    assert_eq!(
        mul(float(2.0), dec("1.5")).unwrap_err().kind,
        ErrorKind::TypeError
    );
    assert_eq!(
        div(dec("1"), float(2.0)).unwrap_err().kind,
        ErrorKind::TypeError
    );
}

#[test]
fn decimal_division_by_zero_is_catchable() {
    assert_eq!(
        div(dec("1"), dec("0")).unwrap_err().kind,
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
fn an_error_concatenates_with_a_str_as_its_message() {
    let err = crate::error::error_value(&DogeError::key_error("no dog"), "s.doge", 1);
    assert!(
        matches!(add(Value::str("caught: "), err.clone()).unwrap(), Value::Str(s) if &*s == "caught: no dog")
    );
    assert!(matches!(add(err, Value::str("!")).unwrap(), Value::Str(s) if &*s == "no dog!"));
}

#[test]
fn errors_are_equal_by_type_message_and_location() {
    let a = crate::error::error_value(&DogeError::overflow("boom"), "s.doge", 4);
    let b = crate::error::error_value(&DogeError::overflow("boom"), "s.doge", 4);
    let c = crate::error::error_value(&DogeError::overflow("boom"), "s.doge", 5);
    assert!(values_equal(&a, &b));
    assert!(!values_equal(&a, &c));
    // Cross-type comparison with an Error is unequal, never an error.
    assert!(!values_equal(&a, &Value::str("boom")));
}

#[test]
fn equality_cross_numeric() {
    assert!(values_equal(&int(1), &float(1.0)));
    assert!(!values_equal(&int(1), &float(1.5)));
    // Int and Decimal compare by value across types.
    assert!(values_equal(&int(2), &dec("2")));
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
    // Decimals order against ints too.
    assert!(matches!(lt(dec("1.5"), int(2)).unwrap(), Value::Bool(true)));
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
fn membership_in_a_list_uses_structural_equality() {
    let xs = Value::list(vec![int(1), int(2), int(3)]);
    assert!(matches!(
        in_(int(2), xs.clone()).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        in_(int(9), xs.clone()).unwrap(),
        Value::Bool(false)
    ));
    // 1 and 1.0 are equal, so an Int needle finds a Float element.
    assert!(matches!(
        in_(int(1), Value::list(vec![float(1.0)])).unwrap(),
        Value::Bool(true)
    ));
}

#[test]
fn membership_in_a_dict_tests_keys() {
    let mut map = crate::ordered_map::OrderedMap::new();
    map.insert("name".to_string(), Value::str("kabosu"));
    let d = Value::dict(map);
    assert!(matches!(
        in_(Value::str("name"), d.clone()).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        in_(Value::str("age"), d.clone()).unwrap(),
        Value::Bool(false)
    ));
    // A non-Str needle can never be a key — absent, not an error.
    assert!(matches!(in_(int(1), d).unwrap(), Value::Bool(false)));
}

#[test]
fn membership_in_a_string_is_substring() {
    let s = Value::str("kabosu");
    assert!(matches!(
        in_(Value::str("bos"), s.clone()).unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        in_(Value::str("xyz"), s.clone()).unwrap(),
        Value::Bool(false)
    ));
    // A non-Str needle against a Str is a catchable type error.
    assert_eq!(in_(int(1), s).unwrap_err().kind, ErrorKind::TypeError);
}

#[test]
fn membership_against_a_non_container_is_a_type_error() {
    assert_eq!(in_(int(5), int(5)).unwrap_err().kind, ErrorKind::TypeError);
    assert_eq!(
        in_(int(1), Value::None).unwrap_err().kind,
        ErrorKind::TypeError
    );
}

#[test]
fn not_in_negates_membership() {
    let xs = Value::list(vec![int(1), int(2)]);
    assert!(matches!(
        not_in(int(2), xs.clone()).unwrap(),
        Value::Bool(false)
    ));
    assert!(matches!(not_in(int(9), xs).unwrap(), Value::Bool(true)));
    // It shares in_'s type rules, so a bad container is the same error.
    assert_eq!(
        not_in(int(5), int(5)).unwrap_err().kind,
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
    assert!(values_equal(&index_get(&xs, &int(-1)).unwrap(), &int(30)));
    assert!(values_equal(&index_get(&xs, &int(-3)).unwrap(), &int(10)));
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
    // An index too large to even be a machine index is out of bounds, not a panic.
    let huge = Value::int(BigInt::from(i64::MAX) * 2);
    assert_eq!(
        index_get(&xs, &huge).unwrap_err().kind,
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
    assert!(values_equal(&index_get(&xs, &int(0)).unwrap(), &int(99)));

    let d = Value::dict(crate::ordered_map::OrderedMap::new());
    index_set(&d, &Value::str("k"), int(7)).unwrap();
    assert!(values_equal(
        &index_get(&d, &Value::str("k")).unwrap(),
        &int(7)
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
    assert!(values_equal(&neg(int(5)).unwrap(), &int(-5)));
    assert!(matches!(neg(float(2.5)).unwrap(), Value::Float(f) if f == -2.5));
    assert!(values_equal(&neg(dec("2.5")).unwrap(), &dec("-2.5")));
    // Negating i64::MIN no longer overflows — it grows past the range.
    let expected = Value::int(-BigInt::from(i64::MIN));
    assert!(values_equal(&neg(int(i64::MIN)).unwrap(), &expected));
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

#[test]
fn iter_value_walks_dict_keys_in_insertion_order() {
    let d = Value::dict_from_pairs(vec![
        (Value::str("name"), Value::str("kabosu")),
        (Value::str("kind"), Value::str("shibe")),
    ])
    .unwrap();
    let snapshot = iter_value(&d).unwrap();
    let keys: Vec<&str> = snapshot
        .iter()
        .map(|k| match k {
            Value::Str(s) => &**s,
            other => panic!("expected Str key, got {other:?}"),
        })
        .collect();
    assert_eq!(keys, ["name", "kind"]);
    // Inserting a key after the snapshot must not grow the snapshot.
    if let Value::Dict(entries) = &d {
        entries.borrow_mut().insert("age".to_string(), int(7));
    }
    assert_eq!(snapshot.len(), 2);
}

#[test]
fn unpack_value_splits_a_list_of_exact_length() {
    let xs = Value::list(vec![int(1), int(2), int(3)]);
    let out = unpack_value(&xs, 3, false).unwrap();
    assert_eq!(out.len(), 3);
    assert!(values_equal(&out[0], &int(1)));
    assert!(values_equal(&out[2], &int(3)));
}

#[test]
fn unpack_value_rejects_a_length_mismatch() {
    let xs = Value::list(vec![int(1), int(2)]);
    assert_eq!(
        unpack_value(&xs, 3, false).unwrap_err().kind,
        ErrorKind::ValueError
    );
    assert_eq!(
        unpack_value(&xs, 1, false).unwrap_err().kind,
        ErrorKind::ValueError
    );
}

#[test]
fn unpack_value_collects_the_rest_into_a_trailing_list() {
    let xs = Value::list(vec![int(1), int(2), int(3), int(4)]);
    let out = unpack_value(&xs, 2, true).unwrap();
    assert_eq!(out.len(), 3);
    assert!(values_equal(&out[0], &int(1)));
    match &out[2] {
        Value::List(rest) => {
            let rest = rest.borrow();
            assert_eq!(rest.len(), 2);
            assert!(values_equal(&rest[0], &int(3)));
        }
        other => panic!("expected a collector list, got {other:?}"),
    }
}

#[test]
fn unpack_value_rest_collects_an_empty_list_when_nothing_is_left() {
    let xs = Value::list(vec![int(1), int(2)]);
    let out = unpack_value(&xs, 2, true).unwrap();
    assert_eq!(out.len(), 3);
    match &out[2] {
        Value::List(rest) => assert!(rest.borrow().is_empty()),
        other => panic!("expected an empty collector list, got {other:?}"),
    }
}

#[test]
fn unpack_value_rest_needs_at_least_the_fixed_count() {
    let xs = Value::list(vec![int(1)]);
    assert_eq!(
        unpack_value(&xs, 2, true).unwrap_err().kind,
        ErrorKind::ValueError
    );
}

#[test]
fn unpack_value_walks_str_characters() {
    let out = unpack_value(&Value::str("hi"), 2, false).unwrap();
    assert!(matches!(&out[0], Value::Str(s) if &**s == "h"));
    assert!(matches!(&out[1], Value::Str(s) if &**s == "i"));
}

#[test]
fn unpack_value_rejects_a_non_iterable() {
    assert_eq!(
        unpack_value(&int(7), 2, false).unwrap_err().kind,
        ErrorKind::TypeError
    );
}

#[test]
fn pow_keeps_ints_and_grows_past_i64() {
    assert!(values_equal(&pow(int(2), int(10)).unwrap(), &int(1024)));
    assert!(values_equal(&pow(int(5), int(0)).unwrap(), &int(1)));
    // A result past the i64 range is exact, never a wraparound or an error.
    let expected = Value::int(BigInt::from(2).pow(64));
    assert!(values_equal(&pow(int(2), int(64)).unwrap(), &expected));
    // Decimals raise to a non-negative Int exponent exactly.
    assert!(values_equal(
        &pow(dec("1.5"), int(2)).unwrap(),
        &dec("2.25")
    ));
}

#[test]
fn pow_promotes_to_float() {
    // A negative exponent or a Float operand yields a Float.
    assert!(matches!(pow(int(2), int(-1)).unwrap(), Value::Float(f) if f == 0.5));
    assert!(matches!(pow(float(2.0), int(3)).unwrap(), Value::Float(f) if f == 8.0));
}

#[test]
fn pow_zero_to_a_negative_power_is_division_by_zero() {
    assert_eq!(
        pow(int(0), int(-1)).unwrap_err().kind,
        ErrorKind::DivisionByZero
    );
}

#[test]
fn pow_on_a_non_number_is_a_type_error() {
    assert_eq!(
        pow(Value::str("x"), int(2)).unwrap_err().kind,
        ErrorKind::TypeError
    );
}

#[test]
fn slice_defaults_and_negative_step() {
    let xs = Value::list(vec![int(10), int(20), int(30), int(40), int(50)]);
    let none = Value::None;
    // xs[1:3]
    let mid = slice_get(&xs, &int(1), &int(3), &none).unwrap();
    assert_eq!(mid.to_string(), "[20, 30]");
    // xs[:] copies the whole list.
    let all = slice_get(&xs, &none, &none, &none).unwrap();
    assert_eq!(all.to_string(), "[10, 20, 30, 40, 50]");
    // xs[-2:] uses a negative start.
    let tail = slice_get(&xs, &int(-2), &none, &none).unwrap();
    assert_eq!(tail.to_string(), "[40, 50]");
    // xs[::-1] reverses.
    let rev = slice_get(&xs, &none, &none, &int(-1)).unwrap();
    assert_eq!(rev.to_string(), "[50, 40, 30, 20, 10]");
}

#[test]
fn slice_clamps_out_of_range_bounds() {
    let xs = Value::list(vec![int(1), int(2), int(3)]);
    let none = Value::None;
    // Bounds past the ends clamp instead of erroring.
    let clamped = slice_get(&xs, &int(-100), &int(100), &none).unwrap();
    assert_eq!(clamped.to_string(), "[1, 2, 3]");
}

#[test]
fn slice_of_a_str_is_character_based() {
    let s = Value::str("héllo");
    let none = Value::None;
    // "héllo"[1:3] spans the two-byte 'é' as one character.
    assert_eq!(
        slice_get(&s, &int(1), &int(3), &none).unwrap().to_string(),
        "él"
    );
    assert_eq!(
        slice_get(&s, &none, &none, &int(-1)).unwrap().to_string(),
        "olléh"
    );
}

#[test]
fn slice_step_zero_and_bad_types_are_catchable() {
    let xs = Value::list(vec![int(1)]);
    let none = Value::None;
    assert_eq!(
        slice_get(&xs, &none, &none, &int(0)).unwrap_err().kind,
        ErrorKind::ValueError
    );
    assert_eq!(
        slice_get(&xs, &Value::str("x"), &none, &none)
            .unwrap_err()
            .kind,
        ErrorKind::TypeError
    );
    // A Dict is not sliceable.
    let dict = Value::dict_from_pairs(vec![]).unwrap();
    assert_eq!(
        slice_get(&dict, &none, &none, &none).unwrap_err().kind,
        ErrorKind::TypeError
    );
}
