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
            | Value::Function(_)
            | Value::Class(_)
            | Value::BoundMethod(_)
            | Value::Error(_),
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
            | Value::Function(_)
            | Value::Class(_)
            | Value::BoundMethod(_)
            | Value::Error(_),
            _,
        ) => Err(DogeError::type_error(format!(
            "cannot index into {}",
            container.describe()
        ))),
    }
}

/// A slice bound (`start`/`end`) as an optional `i64`: the omitted parts of a
/// slice arrive as `Value::None`, an explicit bound must be an Int.
fn slice_bound(what: &str, v: &Value) -> DogeResult<Option<i64>> {
    match v {
        Value::None => Ok(None),
        Value::Int(n) => Ok(Some(*n)),
        _ => Err(DogeError::type_error(format!(
            "a slice {what} must be an Int, not {}",
            v.describe()
        ))),
    }
}

/// The slice step: `None` defaults to `1`, an explicit `0` is a catchable value
/// error, and a non-Int is a type error.
fn slice_step(v: &Value) -> DogeResult<i64> {
    match v {
        Value::None => Ok(1),
        Value::Int(0) => Err(DogeError::value_error("a slice step cannot be zero")),
        Value::Int(n) => Ok(*n),
        _ => Err(DogeError::type_error(format!(
            "a slice step must be an Int, not {}",
            v.describe()
        ))),
    }
}

/// Resolve a slice's `start`, `end`, and `step` against a length into the exact
/// list of indices it selects, clamping out-of-range bounds (Python semantics):
/// negative bounds count from the end, and a negative step walks backward.
fn slice_indices(start: Option<i64>, end: Option<i64>, step: i64, len: usize) -> Vec<usize> {
    let len = len as i64;
    let (lower, upper) = if step < 0 { (-1, len - 1) } else { (0, len) };
    let clamp = |bound: i64| {
        let bound = if bound < 0 { bound + len } else { bound };
        bound.clamp(lower, upper)
    };
    let start = match start {
        Some(s) => clamp(s),
        None => {
            if step < 0 {
                upper
            } else {
                lower
            }
        }
    };
    let end = match end {
        Some(e) => clamp(e),
        None => {
            if step < 0 {
                lower
            } else {
                upper
            }
        }
    };

    let mut indices = Vec::new();
    let mut i = start;
    if step > 0 {
        while i < end {
            indices.push(i as usize);
            i += step;
        }
    } else {
        while i > end {
            indices.push(i as usize);
            i += step;
        }
    }
    indices
}

/// `container[start:end:step]`. A List yields a new List and a Str a new Str
/// (character-based); every other value is a catchable type error. Bounds clamp
/// rather than erroring, matching Python.
pub fn slice_get(container: &Value, start: &Value, end: &Value, step: &Value) -> DogeResult {
    let step = slice_step(step)?;
    let start = slice_bound("start", start)?;
    let end = slice_bound("end", end)?;
    match container {
        Value::List(items) => {
            let items = items.borrow();
            let picked = slice_indices(start, end, step, items.len())
                .into_iter()
                .map(|i| items[i].clone())
                .collect();
            Ok(Value::list(picked))
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let picked: String = slice_indices(start, end, step, chars.len())
                .into_iter()
                .map(|i| chars[i])
                .collect();
            Ok(Value::str(picked))
        }
        // Listed by variant rather than a wildcard, so a new sliceable Value
        // variant forces a decision here.
        Value::Dict(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::Bool(_)
        | Value::None
        | Value::Object(_)
        | Value::Function(_)
        | Value::Class(_)
        | Value::BoundMethod(_)
        | Value::Error(_) => Err(DogeError::type_error(format!(
            "cannot slice {}",
            container.describe()
        ))),
    }
}

/// The sequence a `for` loop walks: a List's elements, a Str's characters, or a
/// Dict's keys in insertion order, captured as an owned snapshot taken when the
/// loop starts. Mutating the original value inside the loop body does not change
/// what the loop visits. Any other value is a catchable type error.
pub fn iter_value(v: &Value) -> DogeResult<Vec<Value>> {
    match v {
        Value::List(items) => Ok(items.borrow().clone()),
        Value::Str(s) => Ok(s.chars().map(|c| Value::str(c.to_string())).collect()),
        Value::Dict(entries) => Ok(entries
            .borrow()
            .iter()
            .map(|(k, _)| Value::str(k))
            .collect()),
        // Listed by variant rather than a wildcard, so a new iterable Value
        // variant forces a decision here.
        Value::Int(_)
        | Value::Float(_)
        | Value::Bool(_)
        | Value::None
        | Value::Object(_)
        | Value::Function(_)
        | Value::Class(_)
        | Value::BoundMethod(_)
        | Value::Error(_) => Err(DogeError::type_error(format!(
            "cannot loop over {}",
            v.describe()
        ))),
    }
}

/// Unpack `v` into the values a multiple-assignment binds: the same sequence a
/// `for` loop walks (a List's elements, a Str's characters, or a Dict's keys),
/// split to `fixed` leading targets plus, when `rest` is set, a trailing
/// collector that gathers every surplus value into a List. The returned Vec has
/// exactly `fixed` elements without a collector, or `fixed + 1` with one (its
/// last element being the collector List). A non-iterable value or a length that
/// cannot fill the targets is a catchable error, so `pls`/`oh no` can handle it.
pub fn unpack_value(v: &Value, fixed: usize, rest: bool) -> DogeResult<Vec<Value>> {
    let mut values = iter_value(v)
        .map_err(|_| DogeError::type_error(format!("cannot unpack {}", v.describe())))?;
    if rest {
        if values.len() < fixed {
            return Err(DogeError::value_error(format!(
                "expected at least {fixed} values to unpack, but got {}",
                values.len()
            )));
        }
        let collected = values.split_off(fixed);
        values.push(Value::list(collected));
    } else if values.len() != fixed {
        return Err(DogeError::value_error(format!(
            "expected {fixed} values to unpack, but got {}",
            values.len()
        )));
    }
    Ok(values)
}
