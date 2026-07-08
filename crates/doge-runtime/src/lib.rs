//! # doge-runtime
//!
//! The behaviour behind every Doge program at runtime: the dynamic [`Value`]
//! type, its operators and indexing, the always-on builtins (`bark`, `len`,
//! `str`/`int`/`float`), and the catchable [`DogeError`]. Generated Rust from
//! the Doge compiler is thin glue that calls into here (CLAUDE.md Hard Rule 5).
//!
//! Two invariants hold everywhere in this crate:
//! - **No `unsafe`** — reference types use `Rc`/`RefCell`, never raw pointers.
//! - **No panics on user-program errors** — every fallible operation returns a
//!   [`DogeResult`] so `pls`/`oh no` can catch it. A panic here would be a
//!   compiler bug, never a Doge user's mistake.

mod builtins;
mod display;
mod error;
mod ops;
mod value;

pub use builtins::{bark, len, to_float, to_int, to_str};
pub use error::{DogeError, DogeResult, ErrorKind};
pub use ops::{
    add, div, eq, floordiv, ge, gt, index_get, index_set, le, lt, mul, ne, neg, not_, rem, sub,
    values_equal,
};
pub use value::Value;
