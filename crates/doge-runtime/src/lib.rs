mod builtins;
mod display;
mod error;
mod functions;
mod objects;
mod ops;
mod stdlib;
mod value;

pub use builtins::{bark, interp, len, range, to_float, to_int, to_str};
pub use error::{bonk_error, enter_call, error_value, exit_call, DogeError, DogeResult, ErrorKind};
pub use functions::{callee_function, cell_get, cell_set, function_arity_error};
pub use objects::{attr_get, attr_set, method_arity_error, no_such_method, object_class_id};
pub use ops::{
    add, div, eq, floordiv, ge, gt, index_get, index_set, iter_value, le, lt, mul, ne, neg, not_,
    rem, sub, values_equal,
};
pub use stdlib::lists::{lists_contains, lists_pop, lists_push, lists_reverse, lists_sort};
pub use stdlib::nerd::{
    nerd_abs, nerd_ceil, nerd_floor, nerd_max, nerd_min, nerd_pow, nerd_round, nerd_sqrt,
};
pub use stdlib::strings::{
    strings_beeg, strings_contains, strings_join, strings_replace, strings_smoll, strings_split,
    strings_trim,
};
pub use value::{Cell, FunctionData, Value};

// Re-exported so the generated glue can build capture cells without importing
// std directly — it only ever writes `use doge_runtime::*;`.
pub use std::cell::RefCell;
pub use std::rc::Rc;
