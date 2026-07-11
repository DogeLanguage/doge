use super::{check_arity, expect_str};
use crate::error::{DogeError, DogeResult};
use crate::value::Value;

pub(super) fn dict_method(recv: &Value, name: &str, mut args: Vec<Value>) -> DogeResult {
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
            let k = expect_str(args.remove(0), "Dict.has needs a Str key")?;
            Ok(Value::Bool(entries.borrow().contains_key(k.as_ref())))
        }
        "remove" => {
            check_arity("Dict", name, 1, argc)?;
            let k = expect_str(args.remove(0), "Dict.remove needs a Str key")?;
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
