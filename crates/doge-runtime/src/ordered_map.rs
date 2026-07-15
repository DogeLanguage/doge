//! The backing store for `Value::Dict`: a string→`Value` map that remembers the
//! order keys were first inserted. Doge dicts are insertion-ordered (like
//! Python's), so iteration, printing, and `keys()`/`values()`/`items()` are all
//! deterministic.
//!
//! It is a `Vec<(String, Value)>` with linear-scan lookups. Script-sized dicts
//! are small, so the simplicity is worth more than the asymptotics an ordered
//! hash map would buy; if dicts ever grow large this is the one place to revisit.

use crate::value::Value;

/// An insertion-ordered string→`Value` map.
#[derive(Debug, Default)]
pub struct OrderedMap {
    entries: Vec<(String, Value)>,
}

impl OrderedMap {
    /// An empty map.
    pub fn new() -> Self {
        OrderedMap {
            entries: Vec::new(),
        }
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The value for `key`, if present.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Whether `key` is present.
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    /// Insert or update. A new key is appended; an existing key keeps its
    /// original position and only its value is replaced (Python semantics).
    pub fn insert(&mut self, key: String, value: Value) {
        if let Some((_, slot)) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            *slot = value;
        } else {
            self.entries.push((key, value));
        }
    }

    /// Remove `key`, returning its value if it was present. The remaining
    /// entries keep their relative order.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let pos = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(pos).1)
    }

    /// Drop every entry.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Iterate entries in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &(String, Value)> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::ToPrimitive;

    #[test]
    fn insertion_order_is_preserved() {
        let mut m = OrderedMap::new();
        m.insert("b".to_string(), Value::int(1));
        m.insert("a".to_string(), Value::int(2));
        m.insert("c".to_string(), Value::int(3));
        let keys: Vec<&str> = m.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["b", "a", "c"]);
    }

    #[test]
    fn reinsert_keeps_position_but_updates_value() {
        let mut m = OrderedMap::new();
        m.insert("x".to_string(), Value::int(1));
        m.insert("y".to_string(), Value::int(2));
        m.insert("x".to_string(), Value::int(9));
        assert_eq!(m.len(), 2);
        let entries: Vec<(&str, i64)> = m
            .iter()
            .map(|(k, v)| match v {
                Value::Int(n) => (k.as_str(), n.to_i64().unwrap()),
                _ => panic!("expected an Int"),
            })
            .collect();
        assert_eq!(entries, [("x", 9), ("y", 2)]);
    }

    #[test]
    fn remove_preserves_remaining_order() {
        let mut m = OrderedMap::new();
        m.insert("a".to_string(), Value::int(1));
        m.insert("b".to_string(), Value::int(2));
        m.insert("c".to_string(), Value::int(3));
        assert!(m
            .remove("b")
            .is_some_and(|v| crate::values_equal(&v, &Value::int(2))));
        assert!(m.remove("z").is_none());
        let keys: Vec<&str> = m.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["a", "c"]);
    }

    #[test]
    fn get_and_contains_and_clear() {
        let mut m = OrderedMap::new();
        m.insert("k".to_string(), Value::str("v"));
        assert!(m.contains_key("k"));
        assert!(matches!(m.get("k"), Some(Value::Str(s)) if &**s == "v"));
        assert!(m.get("nope").is_none());
        m.clear();
        assert!(m.is_empty());
    }
}
