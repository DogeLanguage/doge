//! `fetch` — the file-I/O stdlib module. Every operation takes a Str path; the
//! text operations read and write a Str (so non-text bytes are a catchable
//! IOError), while `read_bytes`/`write_bytes` carry raw Bytes for binary files.
//! Every OS failure — a missing file, a permission problem — is a catchable
//! IOError rather than a panic.

use std::fs;
use std::io::Write;

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
            fetch_read(&Value::Int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
