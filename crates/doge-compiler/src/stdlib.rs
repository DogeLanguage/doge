//! The compiler-side view of the standard library: the modules a `so` import can
//! name, their members, and the `doge-runtime` function each member call wires to.
//! Mirrors the runtime `stdlib` (like [`crate::builtins`] mirrors the builtin
//! functions) — a member here must have a matching `{module}_{member}` function
//! there.

/// One callable member of a module: its arity, the runtime function a call emits,
/// and the call-shape hint shown in arity diagnostics.
pub struct ModuleFn {
    pub name: &'static str,
    pub arity: usize,
    pub runtime_fn: &'static str,
    pub hint: &'static str,
}

/// One importable module: its name, its function members, and its constant
/// members (each a name paired with the Rust expression codegen emits inline).
pub struct Module {
    pub name: &'static str,
    pub funcs: &'static [ModuleFn],
    pub consts: &'static [(&'static str, &'static str)],
}

impl Module {
    /// The function member `name`, if this module has one.
    pub fn func(&self, name: &str) -> Option<&'static ModuleFn> {
        self.funcs.iter().find(|f| f.name == name)
    }

    /// The Rust expression for the constant member `name`, if this module has one.
    pub fn const_expr(&self, name: &str) -> Option<&'static str> {
        self.consts
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, expr)| *expr)
    }

    /// Every member name, comma-joined, for the "unknown member" hint.
    pub fn members(&self) -> String {
        let mut names: Vec<&str> = self.funcs.iter().map(|f| f.name).collect();
        names.extend(self.consts.iter().map(|(n, _)| *n));
        names.join(", ")
    }

    /// The first member name, for hints that show one example call/value.
    pub fn first_member(&self) -> &'static str {
        self.funcs
            .first()
            .map(|f| f.name)
            .or_else(|| self.consts.first().map(|(n, _)| *n))
            .unwrap_or("")
    }
}

/// The runtime function `pack.zoom` maps to. Both engines special-case it: the
/// compiler hands it the pup trampoline plus a globals snapshot, and the
/// interpreter routes it to its own thread-spawning path instead of the generic
/// native dispatch. Kept in step with the `pack` module's `zoom` entry below.
pub const PACK_ZOOM_RUNTIME_FN: &str = "pack_zoom";

/// The module named `name`, if it exists.
pub fn module(name: &str) -> Option<&'static Module> {
    MODULES.iter().find(|m| m.name == name)
}

/// The comma-joined list of module names, for the "no such module" hint.
pub(crate) fn module_names() -> String {
    MODULES
        .iter()
        .map(|m| m.name)
        .collect::<Vec<_>>()
        .join(", ")
}

pub const MODULES: &[Module] = &[
    Module {
        name: "nerd",
        funcs: &[
            ModuleFn {
                name: "abs",
                arity: 1,
                runtime_fn: "nerd_abs",
                hint: "nerd.abs(x)",
            },
            ModuleFn {
                name: "sqrt",
                arity: 1,
                runtime_fn: "nerd_sqrt",
                hint: "nerd.sqrt(x)",
            },
            ModuleFn {
                name: "floor",
                arity: 1,
                runtime_fn: "nerd_floor",
                hint: "nerd.floor(x)",
            },
            ModuleFn {
                name: "ceil",
                arity: 1,
                runtime_fn: "nerd_ceil",
                hint: "nerd.ceil(x)",
            },
            ModuleFn {
                name: "round",
                arity: 1,
                runtime_fn: "nerd_round",
                hint: "nerd.round(x)",
            },
            ModuleFn {
                name: "min",
                arity: 2,
                runtime_fn: "nerd_min",
                hint: "nerd.min(a, b)",
            },
            ModuleFn {
                name: "max",
                arity: 2,
                runtime_fn: "nerd_max",
                hint: "nerd.max(a, b)",
            },
            ModuleFn {
                name: "pow",
                arity: 2,
                runtime_fn: "nerd_pow",
                hint: "nerd.pow(base, exponent)",
            },
        ],
        consts: &[
            ("pi", "Value::Float(std::f64::consts::PI)"),
            ("e", "Value::Float(std::f64::consts::E)"),
        ],
    },
    Module {
        name: "strings",
        funcs: &[
            ModuleFn {
                name: "beeg",
                arity: 1,
                runtime_fn: "strings_beeg",
                hint: "strings.beeg(s)",
            },
            ModuleFn {
                name: "smoll",
                arity: 1,
                runtime_fn: "strings_smoll",
                hint: "strings.smoll(s)",
            },
            ModuleFn {
                name: "trim",
                arity: 1,
                runtime_fn: "strings_trim",
                hint: "strings.trim(s)",
            },
            ModuleFn {
                name: "split",
                arity: 2,
                runtime_fn: "strings_split",
                hint: "strings.split(s, sep)",
            },
            ModuleFn {
                name: "join",
                arity: 2,
                runtime_fn: "strings_join",
                hint: "strings.join(parts, sep)",
            },
            ModuleFn {
                name: "contains",
                arity: 2,
                runtime_fn: "strings_contains",
                hint: "strings.contains(s, needle)",
            },
            ModuleFn {
                name: "replace",
                arity: 3,
                runtime_fn: "strings_replace",
                hint: "strings.replace(s, from, to)",
            },
        ],
        consts: &[],
    },
    Module {
        name: "fetch",
        funcs: &[
            ModuleFn {
                name: "read",
                arity: 1,
                runtime_fn: "fetch_read",
                hint: "fetch.read(path)",
            },
            ModuleFn {
                name: "write",
                arity: 2,
                runtime_fn: "fetch_write",
                hint: "fetch.write(path, text)",
            },
            ModuleFn {
                name: "append",
                arity: 2,
                runtime_fn: "fetch_append",
                hint: "fetch.append(path, text)",
            },
            ModuleFn {
                name: "read_bytes",
                arity: 1,
                runtime_fn: "fetch_read_bytes",
                hint: "fetch.read_bytes(path)",
            },
            ModuleFn {
                name: "write_bytes",
                arity: 2,
                runtime_fn: "fetch_write_bytes",
                hint: "fetch.write_bytes(path, bytes)",
            },
            ModuleFn {
                name: "exists",
                arity: 1,
                runtime_fn: "fetch_exists",
                hint: "fetch.exists(path)",
            },
            ModuleFn {
                name: "delete",
                arity: 1,
                runtime_fn: "fetch_delete",
                hint: "fetch.delete(path)",
            },
        ],
        consts: &[],
    },
    Module {
        name: "env",
        funcs: &[
            ModuleFn {
                name: "args",
                arity: 0,
                runtime_fn: "env_args",
                hint: "env.args()",
            },
            ModuleFn {
                name: "get",
                arity: 1,
                runtime_fn: "env_get",
                hint: "env.get(name)",
            },
        ],
        consts: &[],
    },
    Module {
        name: "howl",
        funcs: &[
            ModuleFn {
                name: "listen",
                arity: 2,
                runtime_fn: "howl_listen",
                hint: "howl.listen(host, port)",
            },
            ModuleFn {
                name: "connect",
                arity: 2,
                runtime_fn: "howl_connect",
                hint: "howl.connect(host, port)",
            },
            ModuleFn {
                name: "accept",
                arity: 1,
                runtime_fn: "howl_accept",
                hint: "howl.accept(listener)",
            },
            ModuleFn {
                name: "port",
                arity: 1,
                runtime_fn: "howl_port",
                hint: "howl.port(sock)",
            },
            ModuleFn {
                name: "send",
                arity: 2,
                runtime_fn: "howl_send",
                hint: "howl.send(conn, text)",
            },
            ModuleFn {
                name: "recv",
                arity: 2,
                runtime_fn: "howl_recv",
                hint: "howl.recv(conn, max_bytes)",
            },
            ModuleFn {
                name: "recv_line",
                arity: 1,
                runtime_fn: "howl_recv_line",
                hint: "howl.recv_line(conn)",
            },
            ModuleFn {
                name: "close",
                arity: 1,
                runtime_fn: "howl_close",
                hint: "howl.close(sock)",
            },
            ModuleFn {
                name: "get",
                arity: 1,
                runtime_fn: "howl_get",
                hint: "howl.get(url)",
            },
            ModuleFn {
                name: "post",
                arity: 2,
                runtime_fn: "howl_post",
                hint: "howl.post(url, body)",
            },
        ],
        consts: &[],
    },
    Module {
        name: "json",
        funcs: &[
            ModuleFn {
                name: "parse",
                arity: 1,
                runtime_fn: "json_parse",
                hint: "json.parse(text)",
            },
            ModuleFn {
                name: "emit",
                arity: 1,
                runtime_fn: "json_emit",
                hint: "json.emit(value)",
            },
        ],
        consts: &[],
    },
    Module {
        name: "dson",
        funcs: &[
            ModuleFn {
                name: "parse",
                arity: 1,
                runtime_fn: "dson_parse",
                hint: "dson.parse(text)",
            },
            ModuleFn {
                name: "emit",
                arity: 1,
                runtime_fn: "dson_emit",
                hint: "dson.emit(value)",
            },
        ],
        consts: &[],
    },
    Module {
        name: "nap",
        funcs: &[
            ModuleFn {
                name: "now",
                arity: 0,
                runtime_fn: "nap_now",
                hint: "nap.now()",
            },
            ModuleFn {
                name: "mono",
                arity: 0,
                runtime_fn: "nap_mono",
                hint: "nap.mono()",
            },
            ModuleFn {
                name: "rest",
                arity: 1,
                runtime_fn: "nap_rest",
                hint: "nap.rest(seconds)",
            },
            ModuleFn {
                name: "stamp",
                arity: 1,
                runtime_fn: "nap_stamp",
                hint: "nap.stamp(secs)",
            },
            ModuleFn {
                name: "parse",
                arity: 1,
                runtime_fn: "nap_parse",
                hint: "nap.parse(text)",
            },
        ],
        consts: &[],
    },
    Module {
        name: "pack",
        funcs: &[
            // `zoom` is special in codegen: it also receives the generated pup
            // trampoline and a snapshot of the globals (see `PACK_ZOOM_RUNTIME_FN`),
            // so its two members here are the two the user actually writes.
            ModuleFn {
                name: "zoom",
                arity: 2,
                runtime_fn: PACK_ZOOM_RUNTIME_FN,
                hint: "pack.zoom(f, [args])",
            },
            ModuleFn {
                name: "fetch",
                arity: 1,
                runtime_fn: "pack_fetch",
                hint: "pack.fetch(pup)",
            },
            ModuleFn {
                name: "bowl",
                arity: 0,
                runtime_fn: "pack_bowl",
                hint: "pack.bowl()",
            },
            ModuleFn {
                name: "drop",
                arity: 2,
                runtime_fn: "pack_drop",
                hint: "pack.drop(bowl, value)",
            },
            ModuleFn {
                name: "sniff",
                arity: 1,
                runtime_fn: "pack_sniff",
                hint: "pack.sniff(bowl)",
            },
        ],
        consts: &[],
    },
    Module {
        name: "roll",
        funcs: &[
            ModuleFn {
                name: "seed",
                arity: 1,
                runtime_fn: "roll_seed",
                hint: "roll.seed(n)",
            },
            ModuleFn {
                name: "int",
                arity: 2,
                runtime_fn: "roll_int",
                hint: "roll.int(low, high)",
            },
            ModuleFn {
                name: "float",
                arity: 0,
                runtime_fn: "roll_float",
                hint: "roll.float()",
            },
            ModuleFn {
                name: "choice",
                arity: 1,
                runtime_fn: "roll_choice",
                hint: "roll.choice(list)",
            },
            ModuleFn {
                name: "shuffle",
                arity: 1,
                runtime_fn: "roll_shuffle",
                hint: "roll.shuffle(list)",
            },
            ModuleFn {
                name: "sample",
                arity: 2,
                runtime_fn: "roll_sample",
                hint: "roll.sample(list, k)",
            },
        ],
        consts: &[],
    },
];
