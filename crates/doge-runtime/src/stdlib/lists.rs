use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use crate::error::{DogeError, DogeResult};
use crate::ops::{order, values_equal};
use crate::value::Value;

/// A List argument as its shared cell, or a catchable type error naming the
/// function.
fn list_ref<'a>(fname: &str, v: &'a Value) -> DogeResult<&'a Rc<RefCell<Vec<Value>>>> {
    match v {
        Value::List(items) => Ok(items),
        _ => Err(DogeError::type_error(format!(
            "lists.{fname} needs a List, got {}",
            v.describe()
        ))),
    }
}

/// `lists.push(xs, item)` — append `item`, returning `none`.
pub fn lists_push(xs: &Value, item: &Value) -> DogeResult {
    list_ref("push", xs)?.borrow_mut().push(item.clone());
    Ok(Value::None)
}

/// `lists.pop(xs)` — remove and return the last element; popping an empty List is
/// a catchable IndexOutOfBounds.
pub fn lists_pop(xs: &Value) -> DogeResult {
    list_ref("pop", xs)?
        .borrow_mut()
        .pop()
        .ok_or_else(|| DogeError::index_out_of_bounds("cannot pop from an empty List"))
}

/// `lists.sort(xs)` — sort in place, returning `none`. The elements must be all
/// Strs or all non-NaN numbers; anything else is a catchable TypeError. The mixed
/// check runs BEFORE sorting so the comparator always sees a total order — a
/// broken order could make std's sort panic, which the runtime never does.
pub fn lists_sort(xs: &Value) -> DogeResult {
    let list = list_ref("sort", xs)?;
    let mut items = list.borrow_mut();
    validate_sortable(&items)?;
    // `order` cannot fail here: validate_sortable guarantees a total order, so
    // the `Equal` fallback is never reached.
    items.sort_by(|a, b| order(a, b).unwrap_or(Ordering::Equal));
    Ok(Value::None)
}

/// Every element must be a Str, or every element a non-NaN number. An empty List
/// is trivially sortable.
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
            "lists.sort needs all Ints/Floats or all Strs",
        ))
    }
}

/// `lists.reverse(xs)` — reverse in place, returning `none`.
pub fn lists_reverse(xs: &Value) -> DogeResult {
    list_ref("reverse", xs)?.borrow_mut().reverse();
    Ok(Value::None)
}

/// `lists.contains(xs, item)` — whether any element equals `item`.
pub fn lists_contains(xs: &Value, item: &Value) -> DogeResult {
    let list = list_ref("contains", xs)?;
    let found = list
        .borrow()
        .iter()
        .any(|element| values_equal(element, item));
    Ok(Value::Bool(found))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn list(items: Vec<Value>) -> Value {
        Value::list(items)
    }

    #[test]
    fn push_and_pop_mutate_in_place() {
        let xs = list(vec![Value::Int(1)]);
        assert!(matches!(
            lists_push(&xs, &Value::Int(2)).unwrap(),
            Value::None
        ));
        assert!(matches!(lists_pop(&xs).unwrap(), Value::Int(2)));
        assert!(matches!(lists_pop(&xs).unwrap(), Value::Int(1)));
        assert_eq!(
            lists_pop(&xs).unwrap_err().kind,
            ErrorKind::IndexOutOfBounds
        );
    }

    #[test]
    fn sort_orders_numbers_and_strings() {
        let nums = list(vec![Value::Int(3), Value::Float(1.5), Value::Int(2)]);
        lists_sort(&nums).unwrap();
        if let Value::List(items) = &nums {
            let items = items.borrow();
            assert!(matches!(items[0], Value::Float(f) if f == 1.5));
            assert!(matches!(items[2], Value::Int(3)));
        }
        let words = list(vec![Value::str("cheems"), Value::str("bonk")]);
        lists_sort(&words).unwrap();
        if let Value::List(items) = &words {
            assert!(matches!(&items.borrow()[0], Value::Str(s) if &**s == "bonk"));
        }
    }

    #[test]
    fn sort_rejects_mixed_types() {
        let mixed = list(vec![Value::Int(1), Value::str("x")]);
        assert_eq!(lists_sort(&mixed).unwrap_err().kind, ErrorKind::TypeError);
    }

    #[test]
    fn reverse_and_contains() {
        let xs = list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        lists_reverse(&xs).unwrap();
        if let Value::List(items) = &xs {
            assert!(matches!(items.borrow()[0], Value::Int(3)));
        }
        assert!(matches!(
            lists_contains(&xs, &Value::Int(2)).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            lists_contains(&xs, &Value::Int(9)).unwrap(),
            Value::Bool(false)
        ));
    }

    #[test]
    fn non_list_subject_is_a_type_error() {
        assert_eq!(
            lists_push(&Value::Int(1), &Value::Int(2)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
