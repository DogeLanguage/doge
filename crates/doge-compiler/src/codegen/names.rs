//! Name-mangling, prefixes, and small string helpers shared across codegen.

pub(super) const ARITY_HEADLINE: &str = "very args. much wrong.";
pub(super) const RUNTIME_ERROR_HEADLINE: &str = "very error. much broken.";

/// Prefix on every generated variable identifier — makes Rust-keyword
/// collisions impossible. Never appears in anything the user sees.
pub(super) const NAME_PREFIX: &str = "v_";
/// Prefix on a function's outer wrapper (`f_greet`): guards recursion depth.
pub(super) const FUNC_PREFIX: &str = "f_";
/// Prefix on a function's body (`b_greet`): the compiled statements. A distinct
/// prefix so a user function named `greet` and one named `b_greet` never clash.
pub(super) const FUNC_BODY_PREFIX: &str = "b_";
/// Prefix on a closure's outer wrapper (`c_3`): a nested function, keyed by its
/// numeric id so the name can never collide with a user function's `f_` pair.
pub(super) const CLOSURE_PREFIX: &str = "c_";
/// Prefix on a closure's body (`cb_3`).
pub(super) const CLOSURE_BODY_PREFIX: &str = "cb_";
/// Prefix on a constructor (`n_0`): builds an instance and runs its `init`.
pub(super) const CTOR_PREFIX: &str = "n_";
/// Prefix on a method's outer wrapper (`mf_0_speak`). The class-id digit means a
/// method name can never collide with a user function's `f_`/`b_` pair.
pub(super) const METHOD_PREFIX: &str = "mf_";
/// Prefix on a method's body (`mb_0_speak`).
pub(super) const METHOD_BODY_PREFIX: &str = "mb_";

/// The name-mangling that keeps every file's top-level names in one flat Rust
/// namespace. The entry (file 0) keeps the original unsuffixed scheme, so
/// single-file output is byte-identical; a module (file N) carries its id right
/// after the prefix. A digit can't start a doge identifier, so `f1_x` can never
/// collide with the entry's `f_` pair for a user name.
pub(super) fn func_wrapper(file_id: u32, name: &str) -> String {
    if file_id == 0 {
        format!("{FUNC_PREFIX}{name}")
    } else {
        format!("{FUNC_PREFIX}{file_id}_{name}")
    }
}

pub(super) fn func_body(file_id: u32, name: &str) -> String {
    if file_id == 0 {
        format!("{FUNC_BODY_PREFIX}{name}")
    } else {
        format!("{FUNC_BODY_PREFIX}{file_id}_{name}")
    }
}

/// The `Env` field name backing a file's top-level binding: the entry's are
/// `v_<name>` (same as a local, since the entry has no name-mangling), a
/// module's constants are `g<id>_<name>`.
pub(super) fn field_name(file_id: u32, name: &str) -> String {
    if file_id == 0 {
        format!("{NAME_PREFIX}{name}")
    } else {
        format!("g{file_id}_{name}")
    }
}

/// Build a comma-joined parameter list: captured cells first (always `Cell`),
/// then value parameters, then the shared `env`. `owned` adds `mut` to value
/// parameters so a body can reassign them; the wrapper takes them plain.
pub(super) fn signature(captures: &[String], params: &[String], owned: bool) -> String {
    let mut parts: Vec<String> = captures
        .iter()
        .map(|c| format!("{NAME_PREFIX}{c}: Cell"))
        .collect();
    parts.extend(params.iter().map(|p| {
        if owned {
            format!("mut {NAME_PREFIX}{p}: Value")
        } else {
            format!("{NAME_PREFIX}{p}: Value")
        }
    }));
    parts.push("env: &mut Env".to_string());
    parts.join(", ")
}

/// Escape a string so it is a valid Rust string-literal body: backslash, quote,
/// newline and tab become their escape sequences. Used for both Str literals and
/// the embedded script path.
pub(super) fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}
