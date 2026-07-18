//! `chase` — run external programs. `chase.run(cmd, args, stdin)` spawns `cmd`
//! with its argument list, optionally feeds it `stdin`, waits for it to finish,
//! and gives back a Dict `{"code", "stdout", "stderr"}`. Every failure to launch
//! the program — a missing binary, a permission problem — is a catchable IOError
//! rather than a panic, and output that is not valid text is an IOError too (as in
//! `fetch`/`howl`). Arguments are type-checked before anything is spawned, so a
//! wrong type never leaves a stray process behind.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::{DogeError, DogeResult};
use crate::ordered_map::OrderedMap;
use crate::stdlib::str_arg;
use crate::value::Value;

/// The exit code reported when a child is terminated by a signal and so has no
/// ordinary exit status of its own.
const SIGNAL_EXIT_CODE: i64 = -1;

/// `chase.run(cmd, args, stdin)` — run `cmd` with the Str `args`, feeding it the
/// Str `stdin` (or nothing when `stdin` is `none`), and give back a Dict
/// `{"code": Int, "stdout": Str, "stderr": Str}`. Failing to launch the program,
/// or output that is not valid text, is a catchable IOError.
pub fn chase_run(cmd: &Value, args: &Value, stdin: &Value) -> DogeResult {
    let cmd = str_arg("chase", "run", cmd)?;
    let args = arg_list(args)?;
    let stdin = stdin_text(stdin)?;

    let mut command = Command::new(cmd);
    command.args(&args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(match stdin {
        Some(_) => Stdio::piped(),
        None => Stdio::null(),
    });

    let mut child = command
        .spawn()
        .map_err(|err| DogeError::io_error(format!("cannot run {cmd}: {err}")))?;

    // Write stdin concurrently to avoid pipe deadlock; an early-closing child
    // produces a normal broken pipe, so the writer ignores write errors.
    let writer = stdin.map(|text| {
        let mut handle = child.stdin.take();
        std::thread::spawn(move || {
            if let Some(pipe) = handle.as_mut() {
                let _ = pipe.write_all(text.as_bytes());
            }
        })
    });

    let output = child
        .wait_with_output()
        .map_err(|err| DogeError::io_error(format!("cannot run {cmd}: {err}")))?;
    if let Some(writer) = writer {
        let _ = writer.join();
    }

    let code = output.status.code().map_or(SIGNAL_EXIT_CODE, i64::from);
    let stdout = decode(cmd, "stdout", output.stdout)?;
    let stderr = decode(cmd, "stderr", output.stderr)?;

    let mut result = OrderedMap::new();
    result.insert("code".to_string(), Value::int(code));
    result.insert("stdout".to_string(), Value::str(stdout));
    result.insert("stderr".to_string(), Value::str(stderr));
    Ok(Value::dict(result))
}

/// The `args` argument as a `Vec<String>`: a List whose every element is a Str.
/// Anything else is a catchable type error.
fn arg_list(args: &Value) -> DogeResult<Vec<String>> {
    let items = match args {
        Value::List(items) => items.borrow(),
        _ => {
            return Err(DogeError::type_error(format!(
                "chase.run needs a List of Str for its args, got {}",
                args.describe()
            )))
        }
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items.iter() {
        match item {
            Value::Str(s) => out.push(s.to_string()),
            other => {
                return Err(DogeError::type_error(format!(
                    "chase.run needs a List of Str for its args, got a {} element",
                    other.describe()
                )))
            }
        }
    }
    Ok(out)
}

/// The `stdin` argument: `Some(text)` for a Str to feed, `None` for `none` (no
/// input). Any other type is a catchable type error.
fn stdin_text(stdin: &Value) -> DogeResult<Option<String>> {
    match stdin {
        Value::Str(s) => Ok(Some(s.to_string())),
        Value::None => Ok(None),
        other => Err(DogeError::type_error(format!(
            "chase.run needs a Str or none for its stdin, got {}",
            other.describe()
        ))),
    }
}

/// Captured output bytes as text, or a catchable IOError naming the stream when
/// they are not valid UTF-8.
fn decode(cmd: &str, stream: &str, bytes: Vec<u8>) -> DogeResult<String> {
    String::from_utf8(bytes)
        .map_err(|_| DogeError::io_error(format!("{cmd} wrote non-text bytes to {stream}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use bigdecimal::ToPrimitive;

    fn run(cmd: &str, args: &[&str], stdin: Value) -> DogeResult {
        let args = Value::list(args.iter().map(Value::str).collect());
        chase_run(&Value::str(cmd), &args, &stdin)
    }

    /// Assert a Str/Int field of the result dict against an expected value,
    /// keeping the `RefCell` borrow local to this helper.
    fn assert_str(dict: &Value, key: &str, expected: &str) {
        match dict {
            Value::Dict(entries) => match entries.borrow().get(key) {
                Some(Value::Str(s)) => assert_eq!(&**s, expected, "{key}"),
                other => panic!("expected a Str {key}, got {other:?}"),
            },
            _ => panic!("expected a dict"),
        }
    }

    fn assert_code(dict: &Value, expected: i64) {
        match dict {
            Value::Dict(entries) => match entries.borrow().get("code") {
                Some(Value::Int(n)) => assert_eq!(n.to_i64().unwrap(), expected, "code"),
                other => panic!("expected an Int code, got {other:?}"),
            },
            _ => panic!("expected a dict"),
        }
    }

    #[test]
    fn captures_stdout_and_a_zero_code() {
        let out = run("printf", &["much wow"], Value::None).unwrap();
        assert_code(&out, 0);
        assert_str(&out, "stdout", "much wow");
        assert_str(&out, "stderr", "");
    }

    #[test]
    fn passes_arguments_through() {
        let out = run("printf", &["%s-%s", "such", "wow"], Value::None).unwrap();
        assert_str(&out, "stdout", "such-wow");
    }

    #[test]
    fn feeds_stdin_to_the_child() {
        let out = run("cat", &[], Value::str("such stdin")).unwrap();
        assert_str(&out, "stdout", "such stdin");
    }

    #[test]
    fn a_child_that_ignores_stdin_does_not_hang_or_error() {
        let out = run("true", &[], Value::str("wasted input")).unwrap();
        assert_code(&out, 0);
    }

    #[test]
    fn reports_a_nonzero_exit_code() {
        let out = run("false", &[], Value::None).unwrap();
        assert_code(&out, 1);
    }

    #[test]
    fn a_missing_program_is_a_catchable_io_error() {
        let err = run("doge-no-such-prog-xyz", &[], Value::None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::IOError);
    }

    #[test]
    fn a_non_str_command_is_a_type_error() {
        let err = chase_run(&Value::int(1), &Value::list(vec![]), &Value::None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
    }

    #[test]
    fn non_list_args_is_a_type_error() {
        let err = chase_run(&Value::str("echo"), &Value::int(1), &Value::None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
    }

    #[test]
    fn a_non_str_args_element_is_a_type_error() {
        let args = Value::list(vec![Value::int(1)]);
        let err = chase_run(&Value::str("echo"), &args, &Value::None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
    }

    #[test]
    fn a_non_str_non_none_stdin_is_a_type_error() {
        let err = chase_run(&Value::str("cat"), &Value::list(vec![]), &Value::int(1)).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
    }
}
