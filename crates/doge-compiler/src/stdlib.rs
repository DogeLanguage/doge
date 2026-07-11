//! The compiler-side view of the standard library: the modules a `so` import can
//! name, their members, and the `doge-runtime` function each member call wires to.
//! Mirrors the runtime `stdlib` (like [`crate::check::BUILTINS`] mirrors the
//! builtin functions) — a member here must have a matching `{module}_{member}`
//! function there.

/// One callable member of a module: its arity, the runtime function a call emits,
/// and the call-shape hint shown in arity diagnostics.
pub(crate) struct ModuleFn {
    pub name: &'static str,
    pub arity: usize,
    pub runtime_fn: &'static str,
    pub hint: &'static str,
}

/// One importable module: its name, its function members, and its constant
/// members (each a name paired with the Rust expression codegen emits inline).
pub(crate) struct Module {
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

/// The module named `name`, if it exists.
pub(crate) fn module(name: &str) -> Option<&'static Module> {
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

pub(crate) const MODULES: &[Module] = &[
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
];
