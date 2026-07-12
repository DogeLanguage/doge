//! `env` — command-line arguments and environment variables. The script's
//! arguments are captured once at startup (`set_script_args`, called from the
//! generated `main` and the interpreter's CLI path) into a process-global slot;
//! `env.args()` reads them back and `env.get(name)` reads the OS environment.

use std::sync::OnceLock;

use crate::error::DogeResult;
use crate::stdlib::str_arg;
use crate::value::Value;

/// The script's command-line arguments (excluding the program name), set once at
/// startup. Unset until `set_script_args` runs, which reads back as no arguments.
static SCRIPT_ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// Record the script's command-line arguments. Called once at program startup
/// (compiled `main` or the interpreter's file runner); any later call is ignored,
/// so the arguments a script sees are fixed for its whole run.
pub fn set_script_args(args: Vec<String>) {
    let _ = SCRIPT_ARGS.set(args);
}

/// `env.args()` — the script's command-line arguments as a List of Str, excluding
/// the program name. Empty when the script was given none.
pub fn env_args() -> DogeResult {
    let args = SCRIPT_ARGS
        .get()
        .map(|args| args.iter().map(Value::str).collect())
        .unwrap_or_default();
    Ok(Value::list(args))
}

/// `env.get(name)` — the value of environment variable `name` as a Str, or `none`
/// when it is unset or not valid text.
pub fn env_get(name: &Value) -> DogeResult {
    let name = str_arg("env", "get", name)?;
    match std::env::var(name) {
        Ok(value) => Ok(Value::str(value)),
        Err(_) => Ok(Value::None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_default_to_empty() {
        // No `set_script_args` runs in this test binary, so the slot is unset.
        match env_args().unwrap() {
            Value::List(items) => assert!(items.borrow().is_empty()),
            _ => panic!("expected a list"),
        }
    }

    #[test]
    fn get_of_an_unset_variable_is_none() {
        let unset = Value::str("DOGE_SURELY_UNSET_VARIABLE_KABOSU");
        assert!(matches!(env_get(&unset).unwrap(), Value::None));
    }
}
