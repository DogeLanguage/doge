//! The compiler-side view of the always-in-scope builtins (`len`, `str`, `int`,
//! `float`, `bytes`, `range`): the runtime function each call wires to, the argument
//! counts it accepts, and how its call is emitted. Mirrors the runtime `builtins`
//! (like [`crate::stdlib`] mirrors the runtime `stdlib`) — a builtin here must
//! have a matching function there. This one table is the single source the
//! checker (name-in-scope, name-clash) and codegen (call emission, value
//! dispatcher) both read.

/// How a builtin call is emitted and dispatched.
#[derive(Clone, Copy)]
pub enum BuiltinShape {
    /// Returns `Result` at runtime, so the emitted call threads `?`/labeled-break
    /// and the value-dispatcher arm forwards the `Result` as-is.
    Fallible,
    /// Cannot fail: the emitted call is used directly, and the value-dispatcher
    /// arm wraps it in `Ok`.
    Infallible,
    /// `range`: one argument (`0..n`) or two (`a..b`), each a fallible `range`
    /// call with the runtime's two-argument shape.
    Range,
    /// `gib`: no argument (read a line) or one (a prompt printed first), a fallible
    /// call that maps to the runtime's `Option<&Value>` prompt shape.
    Prompt,
}

/// One builtin: its name, the `doge-runtime` function a call emits, the argument
/// counts it accepts, its emission shape, and the call-shape hint for arity
/// diagnostics.
pub struct BuiltinFn {
    pub name: &'static str,
    pub runtime_fn: &'static str,
    pub arities: &'static [usize],
    pub shape: BuiltinShape,
    pub hint: &'static str,
}

impl BuiltinFn {
    /// Whether a call with `argc` arguments has a valid arity for this builtin.
    pub fn accepts(&self, argc: usize) -> bool {
        self.arities.contains(&argc)
    }

    /// The accepted-argument phrase for an arity diagnostic, e.g. `1 argument` or
    /// `1 or 2 arguments`.
    pub fn arity_phrase(&self) -> String {
        let counts = self
            .arities
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" or ");
        let noun = if self.arities == [1] {
            "argument"
        } else {
            "arguments"
        };
        format!("{counts} {noun}")
    }
}

pub const BUILTINS: &[BuiltinFn] = &[
    BuiltinFn {
        name: "len",
        runtime_fn: "len",
        arities: &[1],
        shape: BuiltinShape::Fallible,
        hint: "len(thing)",
    },
    BuiltinFn {
        name: "str",
        runtime_fn: "to_str",
        arities: &[1],
        shape: BuiltinShape::Infallible,
        hint: "str(thing)",
    },
    BuiltinFn {
        name: "int",
        runtime_fn: "to_int",
        arities: &[1],
        shape: BuiltinShape::Fallible,
        hint: "int(thing)",
    },
    BuiltinFn {
        name: "float",
        runtime_fn: "to_float",
        arities: &[1],
        shape: BuiltinShape::Fallible,
        hint: "float(thing)",
    },
    BuiltinFn {
        name: "bytes",
        runtime_fn: "to_bytes",
        arities: &[1],
        shape: BuiltinShape::Fallible,
        hint: "bytes(thing)",
    },
    BuiltinFn {
        name: "range",
        runtime_fn: "range",
        arities: &[1, 2],
        shape: BuiltinShape::Range,
        hint: "range(n) or range(a, b)",
    },
    BuiltinFn {
        name: "gib",
        runtime_fn: "gib",
        arities: &[0, 1],
        shape: BuiltinShape::Prompt,
        hint: "gib() or gib(\"prompt\")",
    },
];

/// The builtin named `name`, if there is one.
pub fn builtin(name: &str) -> Option<&'static BuiltinFn> {
    BUILTINS.iter().find(|b| b.name == name)
}

/// Whether `name` is a builtin — always in scope, never redefinable.
pub fn is_builtin(name: &str) -> bool {
    builtin(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names_resolve_and_unknown_do_not() {
        assert!(is_builtin("len"));
        assert!(is_builtin("range"));
        assert!(!is_builtin("nope"));
        assert!(builtin("str").is_some());
    }

    #[test]
    fn arity_phrase_matches_diagnostic_wording() {
        assert_eq!(builtin("len").unwrap().arity_phrase(), "1 argument");
        assert_eq!(builtin("range").unwrap().arity_phrase(), "1 or 2 arguments");
    }

    #[test]
    fn accepts_reflects_declared_arities() {
        let range = builtin("range").unwrap();
        assert!(range.accepts(1));
        assert!(range.accepts(2));
        assert!(!range.accepts(3));
        assert!(!range.accepts(0));

        let len = builtin("len").unwrap();
        assert!(len.accepts(1));
        assert!(!len.accepts(2));
    }
}
