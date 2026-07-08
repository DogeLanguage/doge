use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A dynamically typed Doge value.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    Bool(bool),
    None,
    List(Rc<RefCell<Vec<Value>>>),
    Dict(Rc<RefCell<HashMap<String, Value>>>),
}

impl Value {
    /// Build a `Str` value from anything string-like.
    pub fn str(s: impl AsRef<str>) -> Value {
        Value::Str(Rc::from(s.as_ref()))
    }

    /// Build a `List` value from a vector of elements.
    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    /// Build a `Dict` value from string→value pairs.
    pub fn dict(entries: HashMap<String, Value>) -> Value {
        Value::Dict(Rc::new(RefCell::new(entries)))
    }

    /// Build a `Dict` from key/value pairs evaluated by a dict literal. Every
    /// key must be a `Str`; anything else is a catchable type error. Pairs are
    /// inserted in order, so when a key repeats the last entry wins.
    pub fn dict_from_pairs(pairs: Vec<(Value, Value)>) -> crate::error::DogeResult {
        let mut entries = HashMap::new();
        for (key, value) in pairs {
            match key {
                Value::Str(k) => {
                    entries.insert(k.to_string(), value);
                }
                other => {
                    return Err(crate::error::DogeError::type_error(format!(
                        "dict keys must be a Str, got {}",
                        other.describe()
                    )))
                }
            }
        }
        Ok(Value::dict(entries))
    }

    /// Python-style truthiness: `0`, `0.0`, `""`, empty list/dict, `none` and
    /// `false` are falsy; everything else is truthy.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Bool(b) => *b,
            Value::None => false,
            Value::List(items) => !items.borrow().is_empty(),
            Value::Dict(entries) => !entries.borrow().is_empty(),
        }
    }

    /// The user-facing type name, used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Str(_) => "Str",
            Value::Bool(_) => "Bool",
            Value::None => "None",
            Value::List(_) => "List",
            Value::Dict(_) => "Dict",
        }
    }

    /// The type name with the right English article, for error messages —
    /// `"a Str"`, `"an Int"`. Single source so every diagnostic reads the same.
    pub fn describe(&self) -> String {
        let name = self.type_name();
        let article = match name.chars().next() {
            Some('A' | 'E' | 'I' | 'O' | 'U') => "an",
            _ => "a",
        };
        format!("{article} {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthiness_follows_python() {
        assert!(!Value::Int(0).truthy());
        assert!(Value::Int(1).truthy());
        assert!(!Value::Float(0.0).truthy());
        assert!(Value::Float(0.1).truthy());
        assert!(!Value::str("").truthy());
        assert!(Value::str("dog").truthy());
        assert!(!Value::Bool(false).truthy());
        assert!(Value::Bool(true).truthy());
        assert!(!Value::None.truthy());
        assert!(!Value::list(vec![]).truthy());
        assert!(Value::list(vec![Value::Int(1)]).truthy());
        assert!(!Value::dict(HashMap::new()).truthy());
    }

    #[test]
    fn type_names_match_design() {
        assert_eq!(Value::Int(1).type_name(), "Int");
        assert_eq!(Value::Float(1.0).type_name(), "Float");
        assert_eq!(Value::str("x").type_name(), "Str");
        assert_eq!(Value::Bool(true).type_name(), "Bool");
        assert_eq!(Value::None.type_name(), "None");
        assert_eq!(Value::list(vec![]).type_name(), "List");
        assert_eq!(Value::dict(HashMap::new()).type_name(), "Dict");
    }

    #[test]
    fn describe_uses_the_right_article() {
        assert_eq!(Value::Int(1).describe(), "an Int");
        assert_eq!(Value::str("x").describe(), "a Str");
        assert_eq!(Value::None.describe(), "a None");
    }

    #[test]
    fn dict_from_pairs_last_duplicate_wins() {
        let d = Value::dict_from_pairs(vec![
            (Value::str("k"), Value::Int(1)),
            (Value::str("k"), Value::Int(2)),
        ])
        .unwrap();
        match d {
            Value::Dict(entries) => {
                let entries = entries.borrow();
                assert_eq!(entries.len(), 1);
                assert!(matches!(entries.get("k"), Some(Value::Int(2))));
            }
            _ => panic!("expected a dict"),
        }
    }

    #[test]
    fn dict_from_pairs_rejects_non_str_key() {
        let err = Value::dict_from_pairs(vec![(Value::Int(1), Value::Int(2))]).unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::TypeError);
    }

    #[test]
    fn str_constructor_shares_via_rc() {
        let a = Value::str("kabosu");
        let b = a.clone();
        // Cloning a Str clones the Rc, not the bytes — assignment never "moves".
        match (&a, &b) {
            (Value::Str(x), Value::Str(y)) => assert!(Rc::ptr_eq(x, y)),
            _ => panic!("expected two Str values"),
        }
    }
}
