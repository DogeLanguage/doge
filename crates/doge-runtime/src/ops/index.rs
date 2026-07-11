use crate::error::{DogeError, DogeResult};
use crate::value::Value;

/// Resolve a possibly-negative index against a length, or raise a catchable
/// out-of-bounds error. Negative indices count from the end (`-1` is the last).
fn normalize_index(i: i64, len: usize) -> DogeResult<usize> {
    let len_i = len as i64;
    let idx = if i < 0 { i + len_i } else { i };
    if idx < 0 || idx >= len_i {
        Err(DogeError::index_out_of_bounds(format!(
            "index {i} is out of bounds for length {len}"
        )))
    } else {
        Ok(idx as usize)
    }
}

/// `container[index]`. List by Int, Dict by Str key, Str by character index
/// (never byte index — `"héllo"[1] == "é"`).
pub fn index_get(container: &Value, index: &Value) -> DogeResult {
    match (container, index) {
        (Value::List(items), Value::Int(i)) => {
            let items = items.borrow();
            let idx = normalize_index(*i, items.len())?;
            Ok(items[idx].clone())
        }
        (Value::Str(s), Value::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            let idx = normalize_index(*i, chars.len())?;
            Ok(Value::str(chars[idx].to_string()))
        }
        (Value::Dict(d), Value::Str(k)) => d
            .borrow()
            .get(k.as_ref())
            .cloned()
            .ok_or_else(|| DogeError::key_error(format!("no such key: {k:?}"))),
        (Value::List(_) | Value::Str(_), _) => Err(DogeError::type_error(format!(
            "cannot index {} with {} (need an Int)",
            container.describe(),
            index.describe()
        ))),
        (Value::Dict(_), _) => Err(DogeError::type_error(format!(
            "cannot index a Dict with {} (keys are Str)",
            index.describe()
        ))),
        // Non-container values are not indexable. Listed by variant rather than a
        // wildcard, so a new indexable Value variant forces a decision here.
        (
            Value::Int(_)
            | Value::Float(_)
            | Value::Bool(_)
            | Value::None
            | Value::Object(_)
            | Value::Function(_),
            _,
        ) => Err(DogeError::type_error(format!(
            "cannot index {}",
            container.describe()
        ))),
    }
}

/// `container[index] = value`. List and Dict are mutable in place; Str is
/// immutable, so assigning into one is a catchable type error.
pub fn index_set(container: &Value, index: &Value, value: Value) -> DogeResult<()> {
    match (container, index) {
        (Value::List(items), Value::Int(i)) => {
            let mut items = items.borrow_mut();
            let idx = normalize_index(*i, items.len())?;
            items[idx] = value;
            Ok(())
        }
        (Value::Dict(d), Value::Str(k)) => {
            d.borrow_mut().insert(k.to_string(), value);
            Ok(())
        }
        (Value::Str(_), _) => Err(DogeError::type_error(
            "cannot assign into a Str (Str values are immutable)",
        )),
        (Value::List(_), _) => Err(DogeError::type_error(format!(
            "cannot index a List with {} (need an Int)",
            index.describe()
        ))),
        (Value::Dict(_), _) => Err(DogeError::type_error(format!(
            "cannot index a Dict with {} (keys are Str)",
            index.describe()
        ))),
        // Non-container values cannot be assigned into. Listed by variant rather
        // than a wildcard, so a new assignable Value variant forces a decision.
        (
            Value::Int(_)
            | Value::Float(_)
            | Value::Bool(_)
            | Value::None
            | Value::Object(_)
            | Value::Function(_),
            _,
        ) => Err(DogeError::type_error(format!(
            "cannot index into {}",
            container.describe()
        ))),
    }
}

/// The sequence a `for` loop walks: a List's elements or a Str's characters,
/// captured as an owned snapshot taken when the loop starts. Mutating the
/// original list inside the loop body does not change what the loop visits.
/// Any other value is a catchable type error.
pub fn iter_value(v: &Value) -> DogeResult<Vec<Value>> {
    match v {
        Value::List(items) => Ok(items.borrow().clone()),
        Value::Str(s) => Ok(s.chars().map(|c| Value::str(c.to_string())).collect()),
        // Listed by variant rather than a wildcard, so a new iterable Value
        // variant forces a decision here.
        Value::Int(_)
        | Value::Float(_)
        | Value::Bool(_)
        | Value::None
        | Value::Dict(_)
        | Value::Object(_)
        | Value::Function(_) => Err(DogeError::type_error(format!(
            "cannot loop over {}",
            v.describe()
        ))),
    }
}
