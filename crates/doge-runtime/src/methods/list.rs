use super::sort::sort_list;
use super::{check_arity, expect_int};
use crate::error::{DogeError, DogeResult};
use crate::objects::method_arity_error;
use crate::ops::{slice_contains, values_equal};
use crate::value::Value;

/// Every method name [`list_method`] dispatches, for the bound-method gate
/// (`has_builtin_method`). Kept in step with the `match` below by a unit test.
pub(super) const LIST_METHODS: &[&str] = &[
    "append", "pop", "insert", "remove", "index_of", "contains", "sort", "reverse", "clear",
];

pub(super) fn list_method(
    recv: &Value,
    name: &str,
    mut args: Vec<Value>,
    call: &mut dyn FnMut(Value, Vec<Value>) -> DogeResult,
) -> DogeResult {
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
            let i = expect_int(args.remove(0), "List.insert needs an Int index")?;
            let item = args.remove(0);
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
                Some(p) => Ok(Value::int(p)),
                None => Err(DogeError::value_error("List.index_of: item not found")),
            }
        }
        "contains" => {
            check_arity("List", name, 1, argc)?;
            let target = args.remove(0);
            Ok(Value::Bool(slice_contains(&items.borrow(), &target)))
        }
        "sort" => {
            if argc > 1 {
                return Err(method_arity_error("List", name, 0, Some(1), argc));
            }
            sort_list(items, args.pop(), call)?;
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
