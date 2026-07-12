use std::fmt;

use crate::value::Value;

/// String form of a value as it appears *nested* inside a container: strings
/// gain quotes, everything else prints as it would on its own.
fn repr(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("\"{s}\""),
        other => other.to_string(),
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(x) => {
                // Always show a decimal point so Floats never look like Ints:
                // 3.0 prints "3.0", 2.5 prints "2.5".
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            Value::Str(s) => write!(f, "{s}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::None => write!(f, "none"),
            Value::List(items) => {
                let items = items.borrow();
                let inner = items.iter().map(repr).collect::<Vec<_>>().join(", ");
                write!(f, "[{inner}]")
            }
            Value::Dict(entries) => {
                let entries = entries.borrow();
                let inner = entries
                    .iter()
                    .map(|(k, v)| format!("\"{k}\": {}", repr(v)))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{{{inner}}}")
            }
            Value::Object(o) => write!(f, "<{}>", o.borrow().class_name),
            Value::Function(func) => write!(f, "<function {}>", func.name),
            Value::Class(class) => write!(f, "<class {}>", class.name),
            Value::Error(e) => write!(f, "{}", e.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_format_python_style() {
        assert_eq!(Value::Int(7).to_string(), "7");
        assert_eq!(Value::Float(2.5).to_string(), "2.5");
        assert_eq!(Value::Float(3.0).to_string(), "3.0");
        assert_eq!(Value::str("kabosu").to_string(), "kabosu");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::None.to_string(), "none");
    }

    #[test]
    fn strings_are_bare_at_top_level_but_quoted_when_nested() {
        assert_eq!(Value::str("wow").to_string(), "wow");
        let list = Value::list(vec![Value::str("a"), Value::Int(1)]);
        assert_eq!(list.to_string(), "[\"a\", 1]");
    }

    #[test]
    fn dict_formats_with_quoted_keys_in_insertion_order() {
        let mut map = crate::ordered_map::OrderedMap::new();
        map.insert("name".to_string(), Value::str("kabosu"));
        map.insert("age".to_string(), Value::Int(7));
        assert_eq!(
            Value::dict(map).to_string(),
            "{\"name\": \"kabosu\", \"age\": 7}"
        );
    }

    #[test]
    fn object_prints_its_class_in_angle_brackets() {
        assert_eq!(Value::object(0, "Shibe").to_string(), "<Shibe>");
    }

    #[test]
    fn function_prints_its_name_in_angle_brackets() {
        assert_eq!(
            Value::function(0, "greet", vec![]).to_string(),
            "<function greet>"
        );
    }

    #[test]
    fn error_prints_its_message() {
        let err = crate::error::error_value(
            &crate::error::DogeError::type_error("much wrong"),
            "s.doge",
            2,
        );
        assert_eq!(err.to_string(), "much wrong");
        // Nested in a container it stays bare (it is not a Str), like objects.
        assert_eq!(Value::list(vec![err]).to_string(), "[much wrong]");
    }
}
