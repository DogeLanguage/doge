use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;

use crate::error::{ErrorData, ErrorKind};
use crate::ordered_map::OrderedMap;

/// A shared, mutable binding cell. Closures capture enclosing variables by
/// sharing these: a `such`/param captured by a nested function becomes a `Cell`,
/// so a reassignment on either side is visible to the other.
pub type Cell = Rc<RefCell<Value>>;

/// A dynamically typed Doge value.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    Bool(bool),
    None,
    List(Rc<RefCell<Vec<Value>>>),
    Dict(Rc<RefCell<OrderedMap>>),
    Object(Rc<RefCell<ObjectData>>),
    Function(Rc<FunctionData>),
    /// A class name used as a value: a callable that constructs an instance. It
    /// carries the same [`FunctionData`] a function value does — its `fn_id` is a
    /// constructor arm in the `call_function` dispatcher — so the whole indirect
    /// call path works unchanged, but it keeps a distinct identity (prints
    /// `<class Name>`, type `Class`) rather than masquerading as a function.
    Class(Rc<FunctionData>),
    /// A method bound to its receiver: `such f = a.speak` captures both the
    /// object (or List/Dict) and the method name, so calling `f(...)` dispatches
    /// exactly as `a.speak(...)` would. Name-based, like a direct method call — it
    /// carries no `fn_id`, so both engines route it back through their method
    /// dispatch. Prints `<method Class.name>`, type `Method`, equal only to the
    /// same method bound to the very same receiver.
    BoundMethod(Rc<BoundMethodData>),
    Error(Rc<ErrorData>),
    /// A network socket opened by the `howl` module: a TCP listener or an open
    /// connection, or a closed handle once `howl.close` has run. Sockets are
    /// opaque — they have no methods or fields, compare by identity, and close
    /// automatically when the last reference is dropped. Two socket values are
    /// the same socket only when they share this `Rc`.
    Socket(Rc<SocketData>),
}

/// The innards of a [`Value::Socket`]: the live OS handle behind a `RefCell`, so
/// `howl.recv`/`howl.close` can read and mutate it through a shared value.
#[derive(Debug)]
pub struct SocketData {
    pub state: RefCell<SocketState>,
}

/// What a socket currently is: a listener waiting for connections, an open
/// connection (with any bytes read past a line boundary held for the next read),
/// or a closed handle. Every `howl` operation on a `Closed` socket is a catchable
/// IOError rather than a panic.
#[derive(Debug)]
pub enum SocketState {
    Listener(TcpListener),
    Conn { stream: TcpStream, buf: Vec<u8> },
    Closed,
}

/// A method captured together with the receiver it was read off. Two bound
/// methods are equal only when they name the same method on the very same
/// instance (`a.speak == a.speak`, but not `b.speak`).
#[derive(Debug)]
pub struct BoundMethodData {
    pub receiver: Value,
    pub method: Rc<str>,
}

/// A first-class function value: which compiled function it is (`fn_id`, matched
/// by the generated `call_function` dispatcher), the name it prints and errors
/// under, and the cells it captured from its enclosing scope. Two function values
/// are equal only when they share both the definition and the captured cells.
#[derive(Debug)]
pub struct FunctionData {
    pub fn_id: u32,
    pub name: Rc<str>,
    pub captures: Vec<Cell>,
}

/// The innards of a `many Name:` instance: which class it is (a compile-time id
/// plus the display name) and its fields, which appear the moment they are
/// assigned. Two instances are the same object only when they share this `Rc`.
#[derive(Debug)]
pub struct ObjectData {
    pub class_id: u32,
    pub class_name: Rc<str>,
    pub fields: HashMap<String, Value>,
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

    /// Build a `Dict` value from an insertion-ordered map.
    pub fn dict(entries: OrderedMap) -> Value {
        Value::Dict(Rc::new(RefCell::new(entries)))
    }

    /// Build a fresh instance of the class with `class_id`/`class_name` and no
    /// fields yet — the constructor fills them in with `attr_set`.
    pub fn object(class_id: u32, class_name: &str) -> Value {
        Value::Object(Rc::new(RefCell::new(ObjectData {
            class_id,
            class_name: Rc::from(class_name),
            fields: HashMap::new(),
        })))
    }

    /// Build a caught `Error` value from a raised error's category, message, and
    /// the file/line it was raised at (`err.type` / `err.message` / `err.file` /
    /// `err.line`). Built by [`crate::error::error_value`] at each catch site.
    pub fn error(kind: ErrorKind, message: &str, file: Rc<str>, line: u32) -> Value {
        Value::Error(Rc::new(ErrorData {
            kind,
            message: Rc::from(message),
            file,
            line,
        }))
    }

    /// Build a first-class function value with `fn_id`, display `name`, and the
    /// captured `captures` cells (empty for a top-level function or a closure that
    /// captures nothing).
    pub fn function(fn_id: u32, name: &str, captures: Vec<Cell>) -> Value {
        Value::Function(Rc::new(FunctionData {
            fn_id,
            name: Rc::from(name),
            captures,
        }))
    }

    /// Build a bound-method value capturing `receiver` and the `method` name. The
    /// receiver is any value method dispatch accepts — a `many` instance, or a
    /// List/Dict for its collection methods.
    pub fn bound_method(receiver: Value, method: &str) -> Value {
        Value::BoundMethod(Rc::new(BoundMethodData {
            receiver,
            method: Rc::from(method),
        }))
    }

    /// Build a socket value wrapping an initial [`SocketState`] — a fresh
    /// listener or connection from the `howl` module.
    pub fn socket(state: SocketState) -> Value {
        Value::Socket(Rc::new(SocketData {
            state: RefCell::new(state),
        }))
    }

    /// Build a class value from the constructor arm `fn_id` and the class `name`.
    /// A class captures nothing — calling it always builds a fresh instance — so
    /// its `captures` are empty and two values for the same class compare equal.
    pub fn class(fn_id: u32, name: &str) -> Value {
        Value::Class(Rc::new(FunctionData {
            fn_id,
            name: Rc::from(name),
            captures: Vec::new(),
        }))
    }

    /// Build a `Dict` from key/value pairs evaluated by a dict literal. Every
    /// key must be a `Str`; anything else is a catchable type error. Pairs are
    /// inserted in order, so when a key repeats the last entry wins.
    pub fn dict_from_pairs(pairs: Vec<(Value, Value)>) -> crate::error::DogeResult {
        let mut entries = OrderedMap::new();
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
            Value::Object(_) => true,
            Value::Function(_) => true,
            Value::Class(_) => true,
            Value::BoundMethod(_) => true,
            Value::Error(_) => true,
            Value::Socket(_) => true,
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
            Value::Object(_) => "Object",
            Value::Function(_) => "Function",
            Value::Class(_) => "Class",
            Value::BoundMethod(_) => "Method",
            Value::Error(_) => "Error",
            Value::Socket(_) => "Socket",
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
        assert!(!Value::dict(OrderedMap::new()).truthy());
        // An object is always truthy, even with no fields.
        assert!(Value::object(0, "Shibe").truthy());
        // A function is always truthy.
        assert!(Value::function(0, "greet", vec![]).truthy());
    }

    #[test]
    fn type_names_match_design() {
        assert_eq!(Value::Int(1).type_name(), "Int");
        assert_eq!(Value::Float(1.0).type_name(), "Float");
        assert_eq!(Value::str("x").type_name(), "Str");
        assert_eq!(Value::Bool(true).type_name(), "Bool");
        assert_eq!(Value::None.type_name(), "None");
        assert_eq!(Value::list(vec![]).type_name(), "List");
        assert_eq!(Value::dict(OrderedMap::new()).type_name(), "Dict");
        assert_eq!(Value::object(0, "Shibe").type_name(), "Object");
        assert_eq!(Value::function(0, "greet", vec![]).type_name(), "Function");
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
