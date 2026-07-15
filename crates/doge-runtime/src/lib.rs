mod builtins;
mod display;
mod error;
mod functions;
mod methods;
mod objects;
mod ops;
mod ordered_map;
mod pack;
mod stdlib;
mod value;

pub use builtins::{bark, gib, interp, len, range, to_bytes, to_decimal, to_float, to_int, to_str};
pub use error::{
    assert_error, bonk_error, enter_call, error_field, error_value, exit_call, DogeError,
    DogeResult, ErrorKind,
};
pub use functions::{callee_function, cell_get, cell_set, function_arity_error};
pub use methods::{builtin_method, has_builtin_method};
pub use objects::{
    attr_get, attr_get_or_bind, attr_set, method_arity_error, no_such_method, object_class_id,
};
pub use ops::{
    add, bitand, bitnot, bitor, bitxor, div, eq, floordiv, ge, gt, in_, index_get, index_set,
    iter_value, le, lt, mul, ne, neg, not_, not_in, pow, rem, shl, shr, slice_get, sub,
    unpack_value, values_equal,
};
pub use ordered_map::OrderedMap;
pub use pack::{
    finish_pup, pack_snapshot, pack_value, unpack_globals, unpack_packed, BowlHandle, PackMode,
    Packed, PackedError, PupEntry,
};
pub use stdlib::chase::chase_run;
pub use stdlib::crypto::{crypto_hmac_sha256, crypto_same, crypto_sha256, crypto_token};
pub use stdlib::dson::{dson_emit, dson_parse};
pub use stdlib::env::{env_args, env_get, set_script_args};
pub use stdlib::fetch::{
    fetch_append, fetch_basename, fetch_copy, fetch_delete, fetch_exists, fetch_ext, fetch_join,
    fetch_list, fetch_make_dir, fetch_read, fetch_read_bytes, fetch_remove_dir, fetch_rename,
    fetch_stat, fetch_write, fetch_write_bytes,
};
pub use stdlib::howl::{
    howl_accept, howl_close, howl_connect, howl_get, howl_listen, howl_port, howl_post, howl_recv,
    howl_recv_bytes, howl_recv_line, howl_request, howl_send, howl_send_bytes,
};
pub use stdlib::hunt::{hunt_find, hunt_find_all, hunt_groups, hunt_replace, hunt_test};
pub use stdlib::json::{json_emit, json_parse};
pub use stdlib::nap::{nap_mono, nap_now, nap_parse, nap_rest, nap_stamp};
pub use stdlib::nerd::{
    nerd_abs, nerd_ceil, nerd_floor, nerd_max, nerd_min, nerd_pow, nerd_round, nerd_sqrt,
};
pub use stdlib::pack::{pack_bowl, pack_drop, pack_fetch, pack_sniff, pack_zoom, spawn_pup};
pub use stdlib::roll::{roll_choice, roll_float, roll_int, roll_sample, roll_seed, roll_shuffle};
pub use stdlib::strings::{
    strings_beeg, strings_contains, strings_join, strings_replace, strings_smoll, strings_split,
    strings_trim,
};
pub use value::{BoundMethodData, Cell, FunctionData, Value};

// Re-exported so the generated glue can build capture cells without importing
// std directly — it only ever writes `use doge_runtime::*;`.
pub use std::cell::RefCell;
pub use std::rc::Rc;
