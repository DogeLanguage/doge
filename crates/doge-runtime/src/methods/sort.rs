//! `List.sort`, with or without a key function. Sorting runs on a snapshot of the
//! elements: keys are computed first (the key function is user code and may touch
//! the list), every key is validated so the comparator is a total order, then the
//! (key, element) pairs are stably sorted and written back. A broken order could
//! make std's sort panic, which the runtime never does.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;

use crate::error::{DogeError, DogeResult};
use crate::ops::order;
use crate::value::Value;

/// How deep List keys may nest. A List that contains itself would otherwise make
/// validation recurse forever.
const MAX_KEY_DEPTH: usize = 128;

#[derive(Clone, Copy, PartialEq)]
enum SortClass {
    Number,
    Str,
    List,
}

/// Sort `items` in place by `key` (each element itself when `None`), calling a key
/// function through `call`.
pub(super) fn sort_list(
    items: &Rc<RefCell<Vec<Value>>>,
    key: Option<Value>,
    call: &mut dyn FnMut(Value, Vec<Value>) -> DogeResult,
) -> DogeResult<()> {
    let elements = items.borrow().clone();
    let mut pairs = Vec::with_capacity(elements.len());
    match &key {
        Some(f) => {
            if !matches!(
                f,
                Value::Function(_) | Value::Class(_) | Value::BoundMethod(_)
            ) {
                return Err(DogeError::type_error(format!(
                    "List.sort key must be a function, got {}",
                    f.describe()
                )));
            }
            for element in elements {
                pairs.push((call(f.clone(), vec![element.clone()])?, element));
            }
        }
        None => pairs.extend(elements.into_iter().map(|e| (e.clone(), e))),
    }
    let keys: Vec<Value> = pairs.iter().map(|(k, _)| k.clone()).collect();
    validate_keys(&keys, &mut Vec::new()).map_err(|what| {
        let subject = if key.is_some() {
            "List.sort keys must be"
        } else {
            "List.sort needs"
        };
        DogeError::type_error(format!(
            "{subject} all Ints/Floats, all Strs, or all Lists, but found {what}"
        ))
    })?;
    pairs.sort_by(|a, b| sort_order(&a.0, &b.0));
    *items.borrow_mut() = pairs.into_iter().map(|(_, e)| e).collect();
    Ok(())
}

/// Check that every value in `column` shares one [`SortClass`], recursing into
/// List positions so that item `i` of every List shares one class too. `path` is
/// the List position being checked, for the error. On failure returns what was
/// wrong, e.g. `an Int and a Str at position [1]`.
fn validate_keys(column: &[Value], path: &mut Vec<usize>) -> Result<(), String> {
    let at = || {
        if path.is_empty() {
            String::new()
        } else {
            let indices: String = path.iter().map(|i| format!("[{i}]")).collect();
            format!(" at position {indices}")
        }
    };
    if path.len() > MAX_KEY_DEPTH {
        return Err(format!("Lists nested past {MAX_KEY_DEPTH} levels"));
    }
    let Some(first) = column.first() else {
        return Ok(());
    };
    let class = sort_class(first).ok_or_else(|| format!("{}{}", unsortable(first), at()))?;
    for value in column {
        match sort_class(value) {
            Some(c) if c == class => {}
            Some(_) => {
                return Err(format!(
                    "{} and {}{}",
                    first.describe(),
                    value.describe(),
                    at()
                ))
            }
            None => return Err(format!("{}{}", unsortable(value), at())),
        }
    }
    if class != SortClass::List {
        return Ok(());
    }
    let mut seen = HashSet::new();
    let lists: Vec<Vec<Value>> = column
        .iter()
        .filter_map(|v| match v {
            Value::List(l) if seen.insert(Rc::as_ptr(l)) => Some(l.borrow().clone()),
            _ => None,
        })
        .collect();
    let width = lists.iter().map(Vec::len).max().unwrap_or(0);
    for i in 0..width {
        let next: Vec<Value> = lists.iter().filter_map(|l| l.get(i).cloned()).collect();
        path.push(i);
        validate_keys(&next, path)?;
        path.pop();
    }
    Ok(())
}

fn sort_class(value: &Value) -> Option<SortClass> {
    match value {
        Value::Int(_) => Some(SortClass::Number),
        Value::Float(f) if !f.is_nan() => Some(SortClass::Number),
        Value::Str(_) => Some(SortClass::Str),
        Value::List(_) => Some(SortClass::List),
        _ => None,
    }
}

fn unsortable(value: &Value) -> String {
    match value {
        Value::Float(_) => "a NaN".to_string(),
        other => other.describe(),
    }
}

/// The sort comparator over validated keys: Lists compare item by item, a prefix
/// first; everything else delegates to the `<` ordering. `order` cannot fail
/// here — [`validate_keys`] guarantees a total order — so `Equal` is never used.
fn sort_order(a: &Value, b: &Value) -> Ordering {
    if let (Value::List(x), Value::List(y)) = (a, b) {
        let (x, y) = (x.borrow(), y.borrow());
        return x
            .iter()
            .zip(y.iter())
            .map(|(p, q)| sort_order(p, q))
            .find(|o| o.is_ne())
            .unwrap_or_else(|| x.len().cmp(&y.len()));
    }
    order(a, b).unwrap_or(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn ints(ns: &[i64]) -> Value {
        Value::list(ns.iter().map(|&n| Value::int(n)).collect())
    }

    fn no_calls(_: Value, _: Vec<Value>) -> DogeResult {
        panic!("no key function was given")
    }

    fn sorted(items: Vec<Value>) -> DogeResult<Vec<Value>> {
        let list = Rc::new(RefCell::new(items));
        sort_list(&list, None, &mut no_calls)?;
        let out = list.borrow().clone();
        Ok(out)
    }

    fn same(got: &[Value], want: &[Value]) -> bool {
        got.len() == want.len() && got.iter().zip(want).all(|(g, w)| crate::values_equal(g, w))
    }

    #[test]
    fn lists_compare_item_by_item_with_prefixes_first() {
        let got = sorted(vec![ints(&[2, 1]), ints(&[1, 5]), ints(&[1]), ints(&[])]).unwrap();
        assert!(same(
            &got,
            &[ints(&[]), ints(&[1]), ints(&[1, 5]), ints(&[2, 1])]
        ));
    }

    #[test]
    fn nested_lists_recurse() {
        let a = Value::list(vec![ints(&[1, 2]), Value::str("b")]);
        let b = Value::list(vec![ints(&[1, 1]), Value::str("z")]);
        let got = sorted(vec![a.clone(), b.clone()]).unwrap();
        assert!(same(&got, &[b, a]));
    }

    #[test]
    fn mixed_types_at_a_position_are_a_type_error() {
        let a = Value::list(vec![Value::int(1), Value::str("x")]);
        let b = Value::list(vec![Value::int(2), Value::int(3)]);
        let err = sorted(vec![a, b]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
        assert_eq!(
            err.message,
            "List.sort needs all Ints/Floats, all Strs, or all Lists, but found a Str and an Int at position [1]"
        );
        let err = sorted(vec![Value::int(1), Value::str("x")]).unwrap_err();
        assert_eq!(
            err.message,
            "List.sort needs all Ints/Floats, all Strs, or all Lists, but found an Int and a Str"
        );
    }

    #[test]
    fn nan_and_dicts_are_unsortable() {
        let err = sorted(vec![Value::Float(1.0), Value::Float(f64::NAN)]).unwrap_err();
        assert_eq!(
            err.message,
            "List.sort needs all Ints/Floats, all Strs, or all Lists, but found a NaN"
        );
        let dict = Value::dict(crate::ordered_map::OrderedMap::new());
        let err = sorted(vec![Value::list(vec![dict])]).unwrap_err();
        assert_eq!(
            err.message,
            "List.sort needs all Ints/Floats, all Strs, or all Lists, but found a Dict at position [0]"
        );
    }

    #[test]
    fn a_list_containing_itself_is_an_error_not_a_hang() {
        let xs = Value::list(vec![]);
        if let Value::List(l) = &xs {
            l.borrow_mut().push(xs.clone());
        }
        let err = sorted(vec![xs.clone(), xs]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
        assert!(err.message.contains("Lists nested past 128 levels"));
    }

    #[test]
    fn key_sort_is_stable_and_keys_come_from_the_callback() {
        let records = vec![
            Value::list(vec![Value::str("a"), Value::int(2)]),
            Value::list(vec![Value::str("b"), Value::int(1)]),
            Value::list(vec![Value::str("c"), Value::int(2)]),
        ];
        let list = Rc::new(RefCell::new(records.clone()));
        let mut second = |_: Value, args: Vec<Value>| match &args[0] {
            Value::List(r) => Ok(r.borrow()[1].clone()),
            _ => unreachable!(),
        };
        let key = Value::function(0, "second", vec![]);
        sort_list(&list, Some(key), &mut second).unwrap();
        let want = [records[1].clone(), records[0].clone(), records[2].clone()];
        assert!(same(&list.borrow(), &want));
    }

    #[test]
    fn key_errors_propagate_and_non_functions_are_rejected() {
        let list = Rc::new(RefCell::new(vec![Value::int(1)]));
        let mut failing = |_: Value, _: Vec<Value>| Err(DogeError::value_error("boom"));
        let key = Value::function(0, "f", vec![]);
        let err = sort_list(&list, Some(key), &mut failing).unwrap_err();
        assert_eq!(err.kind, ErrorKind::ValueError);
        assert_eq!(err.message, "boom");
        let err = sort_list(&list, Some(Value::int(3)), &mut no_calls).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
        assert_eq!(err.message, "List.sort key must be a function, got an Int");
    }

    #[test]
    fn a_key_function_that_mutates_the_list_is_overwritten_by_the_sorted_snapshot() {
        let list = Rc::new(RefCell::new(vec![Value::int(2), Value::int(1)]));
        let shared = Rc::clone(&list);
        let mut meddling = |_: Value, args: Vec<Value>| {
            shared.borrow_mut().push(Value::int(99));
            Ok(args[0].clone())
        };
        let key = Value::function(0, "f", vec![]);
        sort_list(&list, Some(key), &mut meddling).unwrap();
        assert!(same(&list.borrow(), &[Value::int(1), Value::int(2)]));
    }
}
