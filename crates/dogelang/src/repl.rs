//! The interactive read-eval-print loop (`doge repl`, or a bare `doge`). Lines are
//! read from stdin and accumulated into a snippet; a snippet that parses cleanly
//! is checked against the session's scope and evaluated by the tree-walking
//! interpreter, so no rustc build happens. A compound construct (a block, an
//! object, a function) spans lines until a blank line runs it, Python-style.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use doge_compiler::{parse_repl, ReplParse};
use doge_interp::Interp;
use doge_runtime::Value;

const BANNER: &str = "much doge. very repl. leave with wow or ctrl-d.";
const PROMPT: &str = "doge> ";
const CONTINUE: &str = "...   ";
/// The path snippets are reported under in diagnostics and error values.
const REPL_PATH: &str = "<repl>";

pub fn run() -> ExitCode {
    println!("{BANNER}");
    let mut interp = Interp::new();
    let stdin = io::stdin();
    let mut buffer = String::new();
    let mut continuing = false;

    prompt(continuing);
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };

        if continuing {
            if line.trim().is_empty() {
                // A blank line ends the compound construct: run whatever we have.
                evaluate(&mut interp, &buffer, true);
                buffer.clear();
                continuing = false;
            } else {
                buffer.push_str(&line);
                buffer.push('\n');
            }
        } else {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                prompt(false);
                continue;
            }
            if trimmed == "wow" {
                break;
            }
            buffer.clear();
            buffer.push_str(&line);
            buffer.push('\n');
            // Not yet complete → keep reading; otherwise it ran (or errored).
            continuing = !evaluate(&mut interp, &buffer, false);
            if !continuing {
                buffer.clear();
            }
        }
        prompt(continuing);
    }

    // A trailing newline so the shell prompt lands on its own line after ctrl-d.
    println!();
    ExitCode::SUCCESS
}

/// Print the primary or continuation prompt, flushing so it shows before input.
fn prompt(continuing: bool) {
    let prompt = if continuing { CONTINUE } else { PROMPT };
    print!("{prompt}");
    let _ = io::stdout().flush();
}

/// Parse the accumulated snippet and, if complete, check and run it. Returns
/// `true` when the snippet was consumed (ran or errored) and `false` when it needs
/// more input. When `force` is set (a blank line ended a compound), an incomplete
/// snippet is reported as an error rather than asking for more.
fn evaluate(interp: &mut Interp, source: &str, force: bool) -> bool {
    match parse_repl(REPL_PATH, source) {
        ReplParse::Complete(script) => {
            run_checked(interp, source, &script);
            true
        }
        ReplParse::Incomplete(diag) => {
            if force {
                eprint!("{}", diag.render());
                true
            } else {
                false
            }
        }
        ReplParse::Error(diag) => {
            eprint!("{}", diag.render());
            true
        }
    }
}

/// Check a parsed snippet against the session scope, then evaluate it — echoing a
/// trailing expression's value and reporting a runtime error in doge-flavored form.
fn run_checked(interp: &mut Interp, source: &str, script: &doge_compiler::Script) {
    let session = interp.session_scope();
    if let Err(diag) = doge_compiler::check_snippet(REPL_PATH, source, script, &session) {
        eprint!("{}", diag.render());
        return;
    }
    match interp.eval_snippet(REPL_PATH, script) {
        Ok(Some(Value::None)) | Ok(None) => {}
        Ok(Some(value)) => println!("{value}"),
        Err(err) => {
            let (_, line) = interp.error_site();
            let src = source
                .lines()
                .nth((line as usize).saturating_sub(1))
                .unwrap_or("");
            eprintln!("very error. much broken.\n\n  {REPL_PATH}:{line}\n    {src}\n  {err}");
        }
    }
}
