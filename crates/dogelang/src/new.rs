//! `doge new <name>`: scaffold a fresh project — a directory with a `doge.toml`
//! manifest, a runnable `main.doge`, and a `.gitignore` for the built binary.

use std::path::Path;
use std::process::ExitCode;

use crate::{EXIT_FAILURE, EXIT_OK};

/// Create a new project directory named `name`, or report why it could not.
pub fn run(name: &str) -> ExitCode {
    if !is_valid_name(name) {
        eprintln!(
            "very name. much invalid.\n\n  {name:?} is not a valid project name — a project is one\n  path segment of letters, digits, - and _ (so it can't escape the\n  current directory or become an odd package name).\n\nsuch fix: try doge new my_app"
        );
        return ExitCode::from(EXIT_FAILURE);
    }

    let dir = Path::new(name);
    if dir.exists() {
        eprintln!(
            "very exists. much occupied.\n\n  {name} already exists — doge won't overwrite it.\n\nsuch fix: pick a new name, or remove {name} first"
        );
        return ExitCode::from(EXIT_FAILURE);
    }

    let manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"main.doge\"\n\n[dependencies]\n"
    );
    let main = format!("bark \"much hello from {name}. very wow.\"\nwow\n");
    let gitignore = format!("# the binary doge build drops here\n/{name}\n");

    let files = [
        ("doge.toml", manifest.as_str()),
        ("main.doge", main.as_str()),
        (".gitignore", gitignore.as_str()),
    ];

    if let Err(err) = std::fs::create_dir_all(dir) {
        eprintln!("very disk. much sad.\n\n  doge could not create {name}: {err}");
        return ExitCode::from(EXIT_FAILURE);
    }
    for (file, contents) in files {
        if let Err(err) = std::fs::write(dir.join(file), contents) {
            eprintln!("very disk. much sad.\n\n  doge could not write {name}/{file}: {err}");
            return ExitCode::from(EXIT_FAILURE);
        }
    }

    println!("such new: created {name}/ — cd {name} && doge bark");
    ExitCode::from(EXIT_OK)
}

/// A project name is a single path segment of letters, digits, `-`, and `_`. This
/// blocks path traversal (`../evil`, `/etc/x`), nested paths (`a/b`), and names
/// that would make an odd package name or `doge.toml` value.
fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names_are_valid() {
        assert!(is_valid_name("my_app"));
        assert!(is_valid_name("doge-lib"));
        assert!(is_valid_name("app2"));
    }

    #[test]
    fn traversal_and_separators_are_rejected() {
        assert!(!is_valid_name(""));
        assert!(!is_valid_name(".."));
        assert!(!is_valid_name("../evil"));
        assert!(!is_valid_name("/etc/passwd"));
        assert!(!is_valid_name("a/b"));
        assert!(!is_valid_name("has space"));
        assert!(!is_valid_name("weird\"quote"));
    }
}
