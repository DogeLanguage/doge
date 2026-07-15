use std::fmt;

use crate::value::Value;

/// The receiver's name in a bound method's display form: a `many` instance shows
/// its class (`<method Shibe.speak>`), a collection its type (`<method
/// List.append>`).
fn receiver_label(v: &Value) -> String {
    match v {
        Value::Object(o) => o.borrow().class_name.to_string(),
        other => other.type_name().to_string(),
    }
}

/// String form of a value as it appears *nested* inside a container: strings
/// gain quotes, everything else prints as it would on its own (the `b"..."` form
/// bytes already print in is self-quoting, so it needs no nested special case).
fn repr(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("\"{s}\""),
        other => other.to_string(),
    }
}

/// The printable `b"..."` form of raw bytes: printable ASCII shown literally
/// (with `"` and `\` escaped), every other byte as a `\xNN` hex escape. Total and
/// UTF-8-safe, so `bark` and `str(bytes)` can render any bytes without decoding.
fn bytes_repr(bytes: &[u8]) -> String {
    let mut out = String::from("b\"");
    for &byte in bytes {
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(byte as char),
            _ => out.push_str(&format!("\\x{byte:02x}")),
        }
    }
    out.push('"');
    out
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
            // A Decimal prints at its own scale, so `dec("0.10")` shows "0.10" —
            // the exact value the user wrote, trailing zeros and all.
            Value::Decimal(d) => write!(f, "{d}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Bytes(b) => write!(f, "{}", bytes_repr(b)),
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
            Value::BoundMethod(m) => {
                write!(f, "<method {}.{}>", receiver_label(&m.receiver), m.method)
            }
            Value::Error(e) => write!(f, "{}", e.message),
            Value::Socket(_) => write!(f, "<socket>"),
            Value::Pup(_) => write!(f, "<pup>"),
            Value::Bowl(_) => write!(f, "<bowl>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_format_python_style() {
        assert_eq!(Value::int(7).to_string(), "7");
        assert_eq!(Value::Float(2.5).to_string(), "2.5");
        assert_eq!(Value::Float(3.0).to_string(), "3.0");
        assert_eq!(Value::str("kabosu").to_string(), "kabosu");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::None.to_string(), "none");
    }

    #[test]
    fn decimals_print_at_their_own_scale() {
        use std::str::FromStr;
        // Trailing zeros are kept — the exact value the user wrote.
        let d = Value::decimal(bigdecimal::BigDecimal::from_str("0.10").unwrap());
        assert_eq!(d.to_string(), "0.10");
        // Nested in a container, a Decimal is bare (it is not a Str).
        assert_eq!(Value::list(vec![d]).to_string(), "[0.10]");
    }

    #[test]
    fn strings_are_bare_at_top_level_but_quoted_when_nested() {
        assert_eq!(Value::str("wow").to_string(), "wow");
        let list = Value::list(vec![Value::str("a"), Value::int(1)]);
        assert_eq!(list.to_string(), "[\"a\", 1]");
    }

    #[test]
    fn dict_formats_with_quoted_keys_in_insertion_order() {
        let mut map = crate::ordered_map::OrderedMap::new();
        map.insert("name".to_string(), Value::str("kabosu"));
        map.insert("age".to_string(), Value::int(7));
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
    fn bound_method_prints_its_receiver_and_name() {
        let obj = Value::object(0, "Shibe");
        assert_eq!(
            Value::bound_method(obj, "speak").to_string(),
            "<method Shibe.speak>"
        );
        let list = Value::list(vec![]);
        assert_eq!(
            Value::bound_method(list, "append").to_string(),
            "<method List.append>"
        );
    }

    #[test]
    fn bytes_print_in_b_quote_form_with_hex_escapes() {
        assert_eq!(Value::bytes("hi").to_string(), "b\"hi\"");
        // Non-printable and high bytes become \xNN; quotes and backslashes escape.
        assert_eq!(
            Value::bytes([0x00, 0xff, b'"', b'\\']).to_string(),
            "b\"\\x00\\xff\\\"\\\\\""
        );
        // Self-quoting, so it stays the same nested in a container.
        assert_eq!(
            Value::list(vec![Value::bytes("hi")]).to_string(),
            "[b\"hi\"]"
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
