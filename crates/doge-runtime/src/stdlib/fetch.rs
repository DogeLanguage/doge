//! `fetch` — the file-I/O stdlib module. Every operation takes a Str path; the
//! text operations read and write a Str (so non-text bytes are a catchable
//! IOError), while `read_bytes`/`write_bytes` carry raw Bytes for binary files.
//! Every OS failure — a missing file, a permission problem — is a catchable
//! IOError rather than a panic.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::error::{DogeError, DogeResult};
use crate::stdlib::{bytes_arg, str_arg};
use crate::value::Value;

/// `fetch.read(path)` — the file's whole contents as a Str. A missing file or one
/// whose bytes are not valid text is a catchable IOError.
pub fn fetch_read(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "read", path)?;
    match fs::read_to_string(path) {
        Ok(text) => Ok(Value::str(text)),
        Err(err) => Err(DogeError::io_error(format!("cannot read {path}: {err}"))),
    }
}

/// `fetch.write(path, text)` — replace the file's contents with `text`, creating
/// it if needed. Returns `none`.
pub fn fetch_write(path: &Value, text: &Value) -> DogeResult {
    let path = str_arg("fetch", "write", path)?;
    let text = str_arg("fetch", "write", text)?;
    match fs::write(path, text) {
        Ok(()) => Ok(Value::None),
        Err(err) => Err(DogeError::io_error(format!("cannot write {path}: {err}"))),
    }
}

/// `fetch.append(path, text)` — add `text` to the end of the file, creating it if
/// needed. Returns `none`.
pub fn fetch_append(path: &Value, text: &Value) -> DogeResult {
    let path = str_arg("fetch", "append", path)?;
    let text = str_arg("fetch", "append", text)?;
    let result = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()));
    match result {
        Ok(()) => Ok(Value::None),
        Err(err) => Err(DogeError::io_error(format!(
            "cannot append to {path}: {err}"
        ))),
    }
}

/// `fetch.read_bytes(path)` — the file's whole contents as raw Bytes, for binary
/// files that are not valid text. A missing file is a catchable IOError.
pub fn fetch_read_bytes(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "read_bytes", path)?;
    match fs::read(path) {
        Ok(bytes) => Ok(Value::bytes(bytes)),
        Err(err) => Err(DogeError::io_error(format!("cannot read {path}: {err}"))),
    }
}

/// `fetch.write_bytes(path, bytes)` — replace the file's contents with the raw
/// `bytes`, creating it if needed. Returns `none`.
pub fn fetch_write_bytes(path: &Value, data: &Value) -> DogeResult {
    let path = str_arg("fetch", "write_bytes", path)?;
    let data = bytes_arg("fetch", "write_bytes", data)?;
    match fs::write(path, data) {
        Ok(()) => Ok(Value::None),
        Err(err) => Err(DogeError::io_error(format!("cannot write {path}: {err}"))),
    }
}

/// `fetch.exists(path)` — whether a file or directory exists at `path`.
pub fn fetch_exists(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "exists", path)?;
    Ok(Value::Bool(fs::metadata(path).is_ok()))
}

/// `fetch.delete(path)` — remove the file at `path`. A missing file is a catchable
/// IOError. Returns `none`.
pub fn fetch_delete(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "delete", path)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(Value::None),
        Err(err) => Err(DogeError::io_error(format!("cannot delete {path}: {err}"))),
    }
}

/// `fetch.list(path)` — the names of the entries in directory `path`, sorted so the
/// order is stable across runs. A missing path or one that is not a directory is a
/// catchable IOError.
pub fn fetch_list(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "list", path)?;
    let entries = fs::read_dir(path)
        .map_err(|err| DogeError::io_error(format!("cannot list {path}: {err}")))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|err| DogeError::io_error(format!("cannot list {path}: {err}")))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(Value::list(names.into_iter().map(Value::str).collect()))
}

/// `fetch.make_dir(path)` — create the directory at `path`, along with any missing
/// parent directories. Doing this when the directory already exists is not an
/// error. Returns `none`.
pub fn fetch_make_dir(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "make_dir", path)?;
    match fs::create_dir_all(path) {
        Ok(()) => Ok(Value::None),
        Err(err) => Err(DogeError::io_error(format!(
            "cannot make directory {path}: {err}"
        ))),
    }
}

/// `fetch.remove_dir(path)` — remove the directory at `path` and everything inside
/// it. A missing path is a catchable IOError. Returns `none`.
pub fn fetch_remove_dir(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "remove_dir", path)?;
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(Value::None),
        Err(err) => Err(DogeError::io_error(format!(
            "cannot remove directory {path}: {err}"
        ))),
    }
}

/// `fetch.rename(from, to)` — move or rename the file or directory `from` to `to`,
/// replacing `to` if it already exists. A missing `from` is a catchable IOError.
/// Returns `none`.
pub fn fetch_rename(from: &Value, to: &Value) -> DogeResult {
    let from = str_arg("fetch", "rename", from)?;
    let to = str_arg("fetch", "rename", to)?;
    match fs::rename(from, to) {
        Ok(()) => Ok(Value::None),
        Err(err) => Err(DogeError::io_error(format!(
            "cannot rename {from} to {to}: {err}"
        ))),
    }
}

/// `fetch.copy(from, to)` — copy the contents of file `from` to `to`, creating or
/// replacing `to`. A missing `from` is a catchable IOError. Returns `none`.
pub fn fetch_copy(from: &Value, to: &Value) -> DogeResult {
    let from = str_arg("fetch", "copy", from)?;
    let to = str_arg("fetch", "copy", to)?;
    match fs::copy(from, to) {
        Ok(_) => Ok(Value::None),
        Err(err) => Err(DogeError::io_error(format!(
            "cannot copy {from} to {to}: {err}"
        ))),
    }
}

/// `fetch.stat(path)` — metadata about `path` as a Dict with `size` (Int bytes),
/// `modified` (Float unix seconds, negative for a pre-epoch time), and `is_dir`
/// (Bool). A missing path, or a platform that cannot report the modified time, is a
/// catchable IOError.
pub fn fetch_stat(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "stat", path)?;
    let meta = fs::metadata(path)
        .map_err(|err| DogeError::io_error(format!("cannot stat {path}: {err}")))?;
    let modified = meta
        .modified()
        .map_err(|err| DogeError::io_error(format!("cannot stat {path}: {err}")))?;
    let modified = match modified.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs_f64(),
        Err(before) => -before.duration().as_secs_f64(),
    };
    Value::dict_from_pairs(vec![
        (Value::str("size"), Value::int(meta.len())),
        (Value::str("modified"), Value::Float(modified)),
        (Value::str("is_dir"), Value::Bool(meta.is_dir())),
    ])
}

/// `fetch.join(a, b)` — join two path segments with the platform separator. If `b`
/// is absolute it replaces `a` entirely, matching how the OS resolves the path.
pub fn fetch_join(a: &Value, b: &Value) -> DogeResult {
    let a = str_arg("fetch", "join", a)?;
    let b = str_arg("fetch", "join", b)?;
    Ok(Value::str(Path::new(a).join(b).to_string_lossy()))
}

/// `fetch.basename(path)` — the final component of `path` (`"a/b/c.txt"` →
/// `"c.txt"`), or `""` when the path has no final component (e.g. `"/"`).
pub fn fetch_basename(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "basename", path)?;
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Value::str(name))
}

/// `fetch.ext(path)` — the extension of `path` including the leading dot
/// (`"c.txt"` → `".txt"`), or `""` when there is none.
pub fn fetch_ext(path: &Value) -> DogeResult {
    let path = str_arg("fetch", "ext", path)?;
    let ext = Path::new(path)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    Ok(Value::str(ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use std::path::PathBuf;

    /// A unique scratch path under the OS temp dir, salted with the process id and
    /// a caller-supplied tag so parallel tests never collide.
    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("doge_fetch_{}_{tag}", std::process::id()))
    }

    #[test]
    fn write_append_read_round_trip() {
        let path = scratch("round_trip");
        let p = Value::str(path.to_string_lossy());
        fetch_write(&p, &Value::str("much ")).unwrap();
        fetch_append(&p, &Value::str("wow")).unwrap();
        assert!(matches!(fetch_read(&p).unwrap(), Value::Str(s) if &*s == "much wow"));
        assert!(matches!(fetch_exists(&p).unwrap(), Value::Bool(true)));
        fetch_delete(&p).unwrap();
        assert!(matches!(fetch_exists(&p).unwrap(), Value::Bool(false)));
    }

    #[test]
    fn write_bytes_read_bytes_round_trip() {
        let path = scratch("bytes_round_trip");
        let p = Value::str(path.to_string_lossy());
        let data = Value::bytes([0x00, 0xff, 0x68, 0x69]);
        fetch_write_bytes(&p, &data).unwrap();
        assert!(matches!(
            fetch_read_bytes(&p).unwrap(),
            Value::Bytes(b) if b[..] == [0x00, 0xff, 0x68, 0x69]
        ));
        fetch_delete(&p).unwrap();
    }

    #[test]
    fn write_bytes_rejects_a_non_bytes_payload() {
        let path = scratch("bytes_bad_payload");
        let p = Value::str(path.to_string_lossy());
        assert_eq!(
            fetch_write_bytes(&p, &Value::str("not bytes"))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn reading_a_missing_file_is_a_catchable_io_error() {
        let path = scratch("missing");
        let err = fetch_read(&Value::str(path.to_string_lossy())).unwrap_err();
        assert_eq!(err.kind, ErrorKind::IOError);
    }

    #[test]
    fn non_str_path_is_a_type_error() {
        assert_eq!(
            fetch_read(&Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }

    /// A unique scratch *directory* under the OS temp dir, salted like `scratch`.
    fn scratch_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("doge_fetch_dir_{}_{tag}", std::process::id()))
    }

    fn str_of(v: Value) -> String {
        match v {
            Value::Str(s) => s.to_string(),
            other => panic!("expected a Str, got {other:?}"),
        }
    }

    #[test]
    fn make_dir_list_stat_remove_round_trip() {
        let dir = scratch_dir("tree");
        let nested = dir.join("sub");
        let d = Value::str(nested.to_string_lossy());
        // create_dir_all makes the parent too, and is idempotent.
        fetch_make_dir(&d).unwrap();
        fetch_make_dir(&d).unwrap();

        let file = Value::str(nested.join("a.txt").to_string_lossy());
        fetch_write(&file, &Value::str("wow")).unwrap();

        let listing = fetch_list(&d).unwrap();
        assert!(matches!(&listing, Value::List(items)
            if items.borrow().len() == 1
                && matches!(&items.borrow()[0], Value::Str(s) if &**s == "a.txt")));

        let info = fetch_stat(&file).unwrap();
        let Value::Dict(map) = &info else {
            panic!("expected a Dict, got {info:?}");
        };
        let map = map.borrow();
        assert!(map
            .get("size")
            .is_some_and(|v| crate::values_equal(v, &Value::int(3))));
        assert!(matches!(map.get("is_dir"), Some(Value::Bool(false))));
        assert!(matches!(map.get("modified"), Some(Value::Float(f)) if *f > 0.0));

        let dir_info = fetch_stat(&Value::str(dir.to_string_lossy())).unwrap();
        let Value::Dict(map) = &dir_info else {
            panic!("expected a Dict, got {dir_info:?}");
        };
        assert!(matches!(
            map.borrow().get("is_dir"),
            Some(Value::Bool(true))
        ));

        fetch_remove_dir(&Value::str(dir.to_string_lossy())).unwrap();
        assert!(matches!(
            fetch_exists(&Value::str(dir.to_string_lossy())).unwrap(),
            Value::Bool(false)
        ));
    }

    #[test]
    fn rename_and_copy_move_file_contents() {
        let dir = scratch_dir("moves");
        fetch_make_dir(&Value::str(dir.to_string_lossy())).unwrap();
        let src = Value::str(dir.join("src.txt").to_string_lossy());
        let renamed = Value::str(dir.join("renamed.txt").to_string_lossy());
        let copied = Value::str(dir.join("copied.txt").to_string_lossy());

        fetch_write(&src, &Value::str("much wow")).unwrap();
        fetch_rename(&src, &renamed).unwrap();
        assert!(matches!(fetch_exists(&src).unwrap(), Value::Bool(false)));
        assert!(matches!(fetch_read(&renamed).unwrap(), Value::Str(s) if &*s == "much wow"));

        fetch_copy(&renamed, &copied).unwrap();
        assert!(matches!(fetch_read(&copied).unwrap(), Value::Str(s) if &*s == "much wow"));
        assert!(matches!(fetch_exists(&renamed).unwrap(), Value::Bool(true)));

        fetch_remove_dir(&Value::str(dir.to_string_lossy())).unwrap();
    }

    #[test]
    fn path_helpers_are_pure_string_ops() {
        assert_eq!(
            str_of(fetch_join(&Value::str("src"), &Value::str("main.doge")).unwrap()),
            "src/main.doge"
        );
        assert_eq!(
            str_of(fetch_basename(&Value::str("a/b/c.txt")).unwrap()),
            "c.txt"
        );
        assert_eq!(str_of(fetch_ext(&Value::str("a/b/c.txt")).unwrap()), ".txt");
        assert_eq!(str_of(fetch_ext(&Value::str("a/b/c")).unwrap()), "");
    }

    #[test]
    fn stat_and_list_on_missing_paths_are_catchable_io_errors() {
        let missing = Value::str(scratch_dir("nope").to_string_lossy());
        assert_eq!(fetch_stat(&missing).unwrap_err().kind, ErrorKind::IOError);
        assert_eq!(fetch_list(&missing).unwrap_err().kind, ErrorKind::IOError);
    }

    #[test]
    fn new_members_reject_non_str_paths() {
        assert_eq!(
            fetch_list(&Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            fetch_join(&Value::str("ok"), &Value::int(1))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
    }
}
