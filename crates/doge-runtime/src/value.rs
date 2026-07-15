use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::thread::JoinHandle;

use bigdecimal::{BigDecimal, Zero};
use num_bigint::BigInt;

use crate::error::{ErrorData, ErrorKind};
use crate::ordered_map::OrderedMap;
use crate::pack::{BowlHandle, Packed, PackedError};

/// A shared, mutable binding cell. Closures capture enclosing variables by
/// sharing these: a `such`/param captured by a nested function becomes a `Cell`,
/// so a reassignment on either side is visible to the other.
pub type Cell = Rc<RefCell<Value>>;

/// A dynamically typed Doge value.
#[derive(Debug, Clone)]
pub enum Value {
    /// An arbitrary-precision integer. Overflow never happens: an operation whose
    /// result outgrows the machine word just keeps more digits, so `Int` behaves as
    /// an unbounded integer to the user. The i64-sized fast path lives inside the
    /// operators, not the type.
    Int(BigInt),
    Float(f64),
    /// An exact base-10 decimal, from `dec(...)`. Unlike `Float` (binary, inexact),
    /// `Decimal` stores value as digits × 10^-scale, so `dec("0.1") + dec("0.2")` is
    /// exactly `dec("0.3")` — the type for money and any exact fractional maths. It
    /// mixes with `Int` (both exact) but not with `Float` (inexact): a `Float`/
    /// `Decimal` arithmetic mix is a catchable `TypeError`.
    Decimal(BigDecimal),
    Str(Rc<str>),
    /// Raw binary data — an immutable, ref-counted byte string, the counterpart of
    /// `Str` for bytes that are not text. Where `Str` is char-based (indexing and
    /// `len` count characters), `Bytes` is byte-based: `bytes[i]` is an `Int`
    /// 0–255 and `len` counts bytes. Produced by `bytes(...)` and the binary
    /// `fetch` reads; decoded back to text with `.decode()`.
    Bytes(Rc<[u8]>),
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
    /// A pup: a function running on its own OS thread, spawned by `pack.zoom`.
    /// Opaque like a socket — no methods or fields, identity comparison — and
    /// waited on with `pack.fetch`, which returns the function's result (or
    /// re-raises the error it hit). A pup cannot be sent to another pup.
    Pup(Rc<PupData>),
    /// A bowl: an unbounded channel opened by `pack.bowl`, over which pups pass
    /// values (`pack.drop`/`pack.sniff`). Unlike every other value, a bowl is
    /// *shared*, not copied, when it crosses a pup boundary — both sides talk over
    /// the same channel. Opaque and compared by identity.
    Bowl(Rc<BowlData>),
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

/// The innards of a [`Value::Pup`]: the join handle of its OS thread behind a
/// `RefCell` so `pack.fetch` can take it. The thread yields either the packed
/// return value or a packed error. Once fetched, the state is [`PupState::Fetched`]
/// and a second fetch is a catchable error.
#[derive(Debug)]
pub struct PupData {
    pub state: RefCell<PupState>,
}

/// What a pup currently is: still running (its join handle is available to wait
/// on), or already fetched (its result has been claimed).
#[derive(Debug)]
pub enum PupState {
    Running(JoinHandle<Result<Packed, PackedError>>),
    Fetched,
}

/// The innards of a [`Value::Bowl`]: the shared channel handle. Cloning a bowl
/// value shares this handle, and so does sending a bowl to a pup — both reach the
/// same channel.
#[derive(Debug)]
pub struct BowlData {
    pub handle: BowlHandle,
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
    /// Build an `Int` value from anything that converts into a `BigInt` — an
    /// `i64`/`u8`/`usize` literal, or a computed `BigInt`. The single construction
    /// helper so call sites never spell `BigInt::from` themselves.
    pub fn int(n: impl Into<BigInt>) -> Value {
        Value::Int(n.into())
    }

    /// Build an `Int` from the decimal-digit string codegen emits for an integer
    /// literal too large to fit an `i64` token. The compiler only ever emits a
    /// valid digit run here, so a parse failure is a compiler bug, not a user error.
    pub fn int_lit(digits: &str) -> Value {
        Value::Int(
            digits
                .parse()
                .expect("compiler bug: emitted an invalid integer literal"),
        )
    }

    /// Build a `Decimal` value from an exact `BigDecimal`.
    pub fn decimal(d: BigDecimal) -> Value {
        Value::Decimal(d)
    }

    /// Build a `Str` value from anything string-like.
    pub fn str(s: impl AsRef<str>) -> Value {
        Value::Str(Rc::from(s.as_ref()))
    }

    /// Build a `Bytes` value from any byte slice.
    pub fn bytes(b: impl AsRef<[u8]>) -> Value {
        Value::Bytes(Rc::from(b.as_ref()))
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

    /// Build a running pup value around the join handle of its OS thread.
    pub fn pup(handle: JoinHandle<Result<Packed, PackedError>>) -> Value {
        Value::Pup(Rc::new(PupData {
            state: RefCell::new(PupState::Running(handle)),
        }))
    }

    /// Build a bowl value around a channel handle — a fresh channel from
    /// `pack.bowl`, or a shared handle rebuilt on the far side of a pup boundary.
    pub fn bowl(handle: BowlHandle) -> Value {
        Value::Bowl(Rc::new(BowlData { handle }))
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
            Value::Int(n) => !n.is_zero(),
            Value::Float(f) => *f != 0.0,
            Value::Decimal(d) => !d.is_zero(),
            Value::Str(s) => !s.is_empty(),
            Value::Bytes(b) => !b.is_empty(),
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
            Value::Pup(_) => true,
            Value::Bowl(_) => true,
        }
    }

    /// The user-facing type name, used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Decimal(_) => "Decimal",
            Value::Str(_) => "Str",
            Value::Bytes(_) => "Bytes",
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
            Value::Pup(_) => "Pup",
            Value::Bowl(_) => "Bowl",
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
        assert!(!Value::int(0).truthy());
        assert!(Value::int(1).truthy());
        assert!(!Value::Float(0.0).truthy());
        assert!(Value::Float(0.1).truthy());
        assert!(!Value::decimal(BigDecimal::from(0)).truthy());
        assert!(Value::decimal(BigDecimal::from(1)).truthy());
        assert!(!Value::str("").truthy());
        assert!(Value::str("dog").truthy());
        assert!(!Value::Bool(false).truthy());
        assert!(Value::Bool(true).truthy());
        assert!(!Value::None.truthy());
        assert!(!Value::list(vec![]).truthy());
        assert!(Value::list(vec![Value::int(1)]).truthy());
        assert!(!Value::dict(OrderedMap::new()).truthy());
        // An object is always truthy, even with no fields.
        assert!(Value::object(0, "Shibe").truthy());
        // A function is always truthy.
        assert!(Value::function(0, "greet", vec![]).truthy());
    }

    #[test]
    fn type_names_match_design() {
        assert_eq!(Value::int(1).type_name(), "Int");
        assert_eq!(Value::Float(1.0).type_name(), "Float");
        assert_eq!(Value::decimal(BigDecimal::from(1)).type_name(), "Decimal");
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
        assert_eq!(Value::int(1).describe(), "an Int");
        assert_eq!(Value::str("x").describe(), "a Str");
        assert_eq!(Value::None.describe(), "a None");
    }

    #[test]
    fn dict_from_pairs_last_duplicate_wins() {
        let d = Value::dict_from_pairs(vec![
            (Value::str("k"), Value::int(1)),
            (Value::str("k"), Value::int(2)),
        ])
        .unwrap();
        match d {
            Value::Dict(entries) => {
                let entries = entries.borrow();
                assert_eq!(entries.len(), 1);
                match entries.get("k") {
                    Some(Value::Int(n)) => assert_eq!(n, &BigInt::from(2)),
                    _ => panic!("expected Int 2"),
                }
            }
            _ => panic!("expected a dict"),
        }
    }

    #[test]
    fn dict_from_pairs_rejects_non_str_key() {
        let err = Value::dict_from_pairs(vec![(Value::int(1), Value::int(2))]).unwrap_err();
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
