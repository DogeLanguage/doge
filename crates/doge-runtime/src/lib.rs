mod builtins;
mod display;
mod error;
mod ops;
mod value;

pub use builtins::{bark, len, range, to_float, to_int, to_str};
pub use error::{DogeError, DogeResult, ErrorKind};
pub use ops::{
    add, div, eq, floordiv, ge, gt, index_get, index_set, iter_value, le, lt, mul, ne, neg, not_,
    rem, sub, values_equal,
};
pub use value::Value;
