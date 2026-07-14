//! The `Send`-able boundary representation a value takes when it crosses a pup
//! (thread) boundary. The runtime is single-threaded by design (`Rc`/`RefCell`),
//! so a `Value` cannot itself move to another thread. [`pack_value`] deep-copies a
//! value into an owned [`Packed`] tree with no `Rc`, which *is* `Send`; the other
//! side rebuilds a fresh `Value` with [`unpack_packed`]. Copying at the boundary
//! is the whole memory model: each pup is its own single-threaded world, so no two
//! threads ever share a mutable cell and no locks or `unsafe` are needed.
//!
//! Two values behave specially because sharing, not copying, is the point:
//! - A **bowl** (channel) is not copied — both sides get a [`BowlHandle`] to the
//!   same channel, so a value dropped on one side can be sniffed on the other.
//! - A **socket** moves in [`PackMode::Transfer`] positions (the sender's handle
//!   becomes closed) and arrives closed in [`PackMode::Snapshot`] positions.
//!
//! A **pup** cannot cross a boundary at all — packing one is a catchable error.

use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};

use crate::error::{DogeError, DogeResult, ErrorKind, ErrorLocation};
use crate::ordered_map::OrderedMap;
use crate::value::{ObjectData, SocketData, SocketState, Value};

/// How deep a value may nest before packing stops with a catchable error — the
/// guard that turns a self-referential value into an error instead of an unbounded
/// recursion. Deliberately far below the call recursion limit: packing may run on
/// the ordinary main-thread stack (whoever calls `pack.zoom`), not the large pup
/// stack, and real data never nests anywhere near this deep.
const PACK_DEPTH_LIMIT: usize = 500;

/// How a socket is treated when its containing value is packed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PackMode {
    /// An explicitly sent position (a `zoom` argument, a `drop` payload): a socket
    /// *moves* — the original handle is replaced with a closed one.
    Transfer,
    /// An ambient position (the globals snapshot, a closure's captures): a socket
    /// is *not* taken from the sender; the pup receives a closed copy instead.
    Snapshot,
}

/// An owned, `Send` mirror of a [`Value`], holding no `Rc` so it can move to
/// another thread. Built by [`pack_value`], consumed by [`unpack_packed`].
#[derive(Debug)]
pub enum Packed {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    List(Vec<Packed>),
    Dict(Vec<(String, Packed)>),
    Object {
        class_id: u32,
        class_name: String,
        fields: Vec<(String, Packed)>,
    },
    Function {
        fn_id: u32,
        name: String,
        captures: Vec<Packed>,
    },
    Class {
        fn_id: u32,
        name: String,
    },
    BoundMethod {
        receiver: Box<Packed>,
        method: String,
    },
    Error {
        kind: ErrorKind,
        message: String,
        file: String,
        line: u32,
    },
    Socket(SocketState),
    Bowl(BowlHandle),
}

/// The clonable half of a bowl: a sender plus a shared receiver over the same
/// channel. Cloning a handle is how both sides of a pup boundary reach one
/// channel — a bowl is deliberately shared, not copied. The channel carries
/// [`Packed`] values, since every message crosses a thread boundary.
#[derive(Clone, Debug)]
pub struct BowlHandle {
    pub(crate) sender: mpsc::Sender<Packed>,
    pub(crate) receiver: Arc<Mutex<mpsc::Receiver<Packed>>>,
}

impl BowlHandle {
    /// A fresh, empty channel.
    pub fn new() -> BowlHandle {
        let (sender, receiver) = mpsc::channel();
        BowlHandle {
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }
}

impl Default for BowlHandle {
    fn default() -> Self {
        BowlHandle::new()
    }
}

/// A [`DogeError`] flattened for the trip back from a pup to its fetcher: no `Rc`,
/// so it is `Send`. Rebuilt into a real error (with its original raise site
/// preserved, so `oh no` sees the same file/line) by [`PackedError::into_error`].
#[derive(Debug)]
pub struct PackedError {
    kind: ErrorKind,
    message: String,
    location: Option<(String, u32)>,
}

impl PackedError {
    /// Flatten an error the pup raised so it can travel back to the fetcher.
    pub fn from_error(err: DogeError) -> PackedError {
        PackedError {
            kind: err.kind,
            message: err.message,
            location: err.location.map(|loc| (loc.file.to_string(), loc.line)),
        }
    }

    /// Rebuild the error on the fetching side, so `pack.fetch` re-raises exactly
    /// what the pup raised.
    pub fn into_error(self) -> DogeError {
        DogeError {
            kind: self.kind,
            message: self.message,
            location: self.location.map(|(file, line)| ErrorLocation {
                file: Rc::from(file),
                line,
            }),
        }
    }
}

/// The signature of the generated trampoline a pup runs: build a fresh world from
/// the globals snapshot, unpack the callee and its arguments, run the call, and
/// pack the result (or the error) for the trip home. A plain `fn` pointer, so it
/// is `Send` and can be handed to a spawned thread.
pub type PupEntry = fn(Packed, Packed, Vec<Packed>) -> Result<Packed, PackedError>;

/// Deep-copy a value into its `Send` boundary form. A `Pup` cannot cross (a
/// catchable type error), and a structure nested past [`PACK_DEPTH_LIMIT`] — which
/// a self-referential value hits — is a catchable value error rather than an
/// unbounded copy. `mode` decides only how a socket is treated.
pub fn pack_value(value: &Value, mode: PackMode) -> DogeResult<Packed> {
    pack_at(value, mode, 0)
}

/// Snapshot a value for an ambient position (the globals a pup inherits), never
/// failing: a value that cannot be packed — a `Pup`, or one nested too deep —
/// simply arrives as `none` in the pup rather than blocking the spawn, since the
/// pup likely never touches that binding.
pub fn pack_snapshot(value: &Value) -> Packed {
    pack_value(value, PackMode::Snapshot).unwrap_or(Packed::None)
}

fn pack_at(value: &Value, mode: PackMode, depth: usize) -> DogeResult<Packed> {
    if depth >= PACK_DEPTH_LIMIT {
        return Err(DogeError::value_error(
            "cannot send a value nested that deeply (or referring to itself) to a pup",
        ));
    }
    let next = depth + 1;
    Ok(match value {
        Value::Int(n) => Packed::Int(*n),
        Value::Float(f) => Packed::Float(*f),
        Value::Str(s) => Packed::Str(s.to_string()),
        Value::Bool(b) => Packed::Bool(*b),
        Value::None => Packed::None,
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.borrow().len());
            for item in items.borrow().iter() {
                out.push(pack_at(item, mode, next)?);
            }
            Packed::List(out)
        }
        Value::Dict(entries) => {
            let entries = entries.borrow();
            let mut out = Vec::with_capacity(entries.len());
            for (key, val) in entries.iter() {
                out.push((key.to_string(), pack_at(val, mode, next)?));
            }
            Packed::Dict(out)
        }
        Value::Object(obj) => {
            let obj = obj.borrow();
            let mut fields = Vec::with_capacity(obj.fields.len());
            for (name, val) in obj.fields.iter() {
                fields.push((name.clone(), pack_at(val, mode, next)?));
            }
            Packed::Object {
                class_id: obj.class_id,
                class_name: obj.class_name.to_string(),
                fields,
            }
        }
        Value::Function(func) => {
            let mut captures = Vec::with_capacity(func.captures.len());
            for cell in func.captures.iter() {
                captures.push(pack_at(&cell.borrow(), mode, next)?);
            }
            Packed::Function {
                fn_id: func.fn_id,
                name: func.name.to_string(),
                captures,
            }
        }
        Value::Class(class) => Packed::Class {
            fn_id: class.fn_id,
            name: class.name.to_string(),
        },
        Value::BoundMethod(method) => Packed::BoundMethod {
            receiver: Box::new(pack_at(&method.receiver, mode, next)?),
            method: method.method.to_string(),
        },
        Value::Error(err) => Packed::Error {
            kind: err.kind,
            message: err.message.to_string(),
            file: err.file.to_string(),
            line: err.line,
        },
        Value::Socket(socket) => Packed::Socket(pack_socket(socket, mode)),
        Value::Bowl(bowl) => Packed::Bowl(bowl.handle.clone()),
        Value::Pup(_) => {
            return Err(DogeError::type_error("cannot send a Pup to another pup"));
        }
    })
}

/// A socket's state for the pup: taken (leaving a closed handle behind) when it is
/// explicitly transferred, or a fresh closed handle when it is merely snapshotted.
fn pack_socket(socket: &Rc<SocketData>, mode: PackMode) -> SocketState {
    match mode {
        PackMode::Transfer => socket.state.replace(SocketState::Closed),
        PackMode::Snapshot => SocketState::Closed,
    }
}

/// Rebuild a fresh [`Value`] from its boundary form on the receiving thread, with
/// brand-new `Rc`s and cells — no sharing survives the trip, which is exactly the
/// copy semantics a pup boundary promises.
pub fn unpack_packed(packed: Packed) -> Value {
    match packed {
        Packed::Int(n) => Value::Int(n),
        Packed::Float(f) => Value::Float(f),
        Packed::Str(s) => Value::str(s),
        Packed::Bool(b) => Value::Bool(b),
        Packed::None => Value::None,
        Packed::List(items) => Value::list(items.into_iter().map(unpack_packed).collect()),
        Packed::Dict(pairs) => {
            let mut entries = OrderedMap::new();
            for (key, val) in pairs {
                entries.insert(key, unpack_packed(val));
            }
            Value::dict(entries)
        }
        Packed::Object {
            class_id,
            class_name,
            fields,
        } => {
            let fields = fields
                .into_iter()
                .map(|(name, val)| (name, unpack_packed(val)))
                .collect();
            Value::Object(Rc::new(std::cell::RefCell::new(ObjectData {
                class_id,
                class_name: Rc::from(class_name.as_str()),
                fields,
            })))
        }
        Packed::Function {
            fn_id,
            name,
            captures,
        } => {
            let cells = captures
                .into_iter()
                .map(|c| Rc::new(std::cell::RefCell::new(unpack_packed(c))))
                .collect();
            Value::function(fn_id, &name, cells)
        }
        Packed::Class { fn_id, name } => Value::class(fn_id, &name),
        Packed::BoundMethod { receiver, method } => {
            Value::bound_method(unpack_packed(*receiver), &method)
        }
        Packed::Error {
            kind,
            message,
            file,
            line,
        } => Value::error(kind, &message, Rc::from(file.as_str()), line),
        Packed::Socket(state) => Value::socket(state),
        Packed::Bowl(handle) => Value::bowl(handle),
    }
}

/// Unpack a globals snapshot into the values a pup's fresh `Env` fields take, in
/// the order they were packed. Anything that is not a list snapshot yields no
/// values, so the fields fall back to `none`.
pub fn unpack_globals(globals: Packed) -> Vec<Value> {
    match globals {
        Packed::List(items) => items.into_iter().map(unpack_packed).collect(),
        _ => Vec::new(),
    }
}

/// Pack a call's outcome for the trip back from a pup to its fetcher: the return
/// value is *transferred* (the pup is finishing, so a socket in the result moves
/// home), and a raised error is flattened. A return value that cannot be packed
/// becomes the fetched error.
pub fn finish_pup(result: DogeResult<Value>) -> Result<Packed, PackedError> {
    match result {
        Ok(value) => pack_value(&value, PackMode::Transfer).map_err(PackedError::from_error),
        Err(err) => Err(PackedError::from_error(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(value: &Value) -> Value {
        unpack_packed(pack_value(value, PackMode::Transfer).unwrap())
    }

    #[test]
    fn scalars_and_collections_survive_a_round_trip() {
        let mut map = OrderedMap::new();
        map.insert("k".to_string(), Value::Int(1));
        let value = Value::list(vec![
            Value::Int(7),
            Value::Float(2.5),
            Value::str("wow"),
            Value::Bool(true),
            Value::None,
            Value::dict(map),
        ]);
        assert!(crate::values_equal(&value, &round_trip(&value)));
    }

    #[test]
    fn a_copy_shares_nothing_with_the_original() {
        let inner = Value::list(vec![Value::Int(1)]);
        let copy = round_trip(&inner);
        // Mutating the copy must not touch the original — the trip severed sharing.
        if let Value::List(items) = &copy {
            items.borrow_mut().push(Value::Int(2));
        }
        let Value::List(original) = &inner else {
            unreachable!()
        };
        assert_eq!(original.borrow().len(), 1);
    }

    #[test]
    fn a_pup_cannot_be_packed() {
        // A bowl stands in for any un-sendable handle here; a real Pup needs a
        // spawned thread, but the type-error path is what matters.
        let err = pack_value(&fake_pup(), PackMode::Transfer).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
    }

    fn fake_pup() -> Value {
        // A pup whose thread has already finished, built without spawning.
        let handle = std::thread::spawn(|| Ok(Packed::None));
        Value::pup(handle)
    }

    #[test]
    fn a_deeply_nested_value_is_a_catchable_error_not_a_hang() {
        // A list that contains itself: packing must stop with an error, not recurse
        // forever. Run on a generous stack so the depth guard — not a stack
        // overflow — is what stops it, exactly as it would in a real program.
        let handle = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let list = Value::list(vec![]);
                if let Value::List(items) = &list {
                    items.borrow_mut().push(list.clone());
                }
                pack_value(&list, PackMode::Transfer).unwrap_err().kind
            })
            .unwrap();
        assert_eq!(handle.join().unwrap(), ErrorKind::ValueError);
    }

    #[test]
    fn transfer_moves_a_socket_but_snapshot_leaves_it_open() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds a loopback port");
        let socket = Value::socket(SocketState::Listener(listener));

        // A snapshot leaves the original socket untouched (still a live listener).
        let _ = pack_value(&socket, PackMode::Snapshot).unwrap();
        let Value::Socket(handle) = &socket else {
            unreachable!()
        };
        assert!(
            matches!(&*handle.state.borrow(), SocketState::Listener(_)),
            "snapshot leaves the original open"
        );

        // A transfer takes the live handle, leaving the original closed.
        let _ = pack_value(&socket, PackMode::Transfer).unwrap();
        assert!(
            matches!(&*handle.state.borrow(), SocketState::Closed),
            "transfer closes the original"
        );
    }
}
