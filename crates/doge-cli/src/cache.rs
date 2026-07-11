use std::path::{Path, PathBuf};

/// FNV-1a 64-bit offset basis and prime (the canonical constants).
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Bumped when the generated-code shape changes without a crate version bump, so
/// binaries cached by an older milestone rebuild instead of running stale.
const CODEGEN_REV: &str = "m6";

/// The message shown when no writable cache location can be found.
const NO_CACHE_HOME: &str = "\
very homeless. much confuse.

  doge could not find a place to cache builds — no DOGE_CACHE_DIR, XDG_CACHE_HOME, or HOME is set.

such fix: set DOGE_CACHE_DIR to a writable directory";

/// The classic FNV-1a hash: XOR each byte into the accumulator, then multiply by
/// the prime (wrapping).
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The 16-hex-digit cache key for a script. Salted with the crate version, the
/// codegen revision, and a hash of the `doge-runtime` source (from `build.rs`),
/// so any of those changing rebuilds instead of running a stale binary.
fn cache_key(source: &str) -> String {
    cache_key_from(
        env!("CARGO_PKG_VERSION"),
        CODEGEN_REV,
        env!("DOGE_RUNTIME_HASH"),
        source,
    )
}

/// The cache key from its explicit inputs, so the salting can be tested without
/// depending on the compiled-in constants.
fn cache_key_from(version: &str, codegen_rev: &str, runtime_hash: &str, source: &str) -> String {
    let mut buf = Vec::with_capacity(source.len() + 48);
    for salt in [version, codegen_rev, runtime_hash] {
        buf.extend_from_slice(salt.as_bytes());
        buf.push(0);
    }
    buf.extend_from_slice(source.as_bytes());
    format!("{:016x}", fnv1a_64(&buf))
}

/// Where the cache lives: `$DOGE_CACHE_DIR`, else `$XDG_CACHE_HOME/doge`, else
/// `$HOME/.cache/doge`. Tests set `DOGE_CACHE_DIR` to stay out of the real cache.
fn cache_root() -> Result<PathBuf, String> {
    for (var, suffix) in [
        ("DOGE_CACHE_DIR", None),
        ("XDG_CACHE_HOME", Some("doge")),
        ("HOME", Some(".cache/doge")),
    ] {
        if let Ok(value) = std::env::var(var) {
            if !value.is_empty() {
                let mut root = PathBuf::from(value);
                if let Some(suffix) = suffix {
                    root.push(suffix);
                }
                return Ok(root);
            }
        }
    }
    Err(NO_CACHE_HOME.to_string())
}

/// The resolved cache locations for one script. The package name carries the
/// hash so different scripts never collide inside the shared target dir.
pub struct CachePaths {
    /// `<root>/scripts/<hash>/` — holds Cargo.toml, src/main.rs, source.doge.
    pub entry_dir: PathBuf,
    /// `<root>/target` — shared, so `doge-runtime` compiles once for all scripts.
    pub target_dir: PathBuf,
    /// `<root>/target/release/doge_script_<hash>` (plus `.exe` on Windows).
    pub binary: PathBuf,
    /// `doge_script_<hash>` — the Cargo package and binary name.
    pub package: String,
}

/// Resolve every cache path for `source`, or a rendered error if no cache home
/// exists.
pub fn resolve(source: &str) -> Result<CachePaths, String> {
    let root = cache_root()?;
    let hash = cache_key(source);
    let package = format!("doge_script_{hash}");
    let entry_dir = root.join("scripts").join(&hash);
    let target_dir = root.join("target");
    let binary = target_dir
        .join("release")
        .join(format!("{package}{}", std::env::consts::EXE_SUFFIX));
    Ok(CachePaths {
        entry_dir,
        target_dir,
        binary,
        package,
    })
}

/// A cache hit needs both the built binary AND a stored source that reads back
/// byte-identical — this guards against hash collisions and torn writes.
pub fn cache_hit(entry_dir: &Path, binary: &Path, source: &str) -> bool {
    binary.exists()
        && std::fs::read_to_string(entry_dir.join("source.doge"))
            .map(|stored| stored == source)
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_matches_known_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63dc4c8601ec8c);
    }

    #[test]
    fn cache_key_changes_with_source() {
        assert_ne!(cache_key("such x = 1\n"), cache_key("such x = 2\n"));
    }

    #[test]
    fn cache_key_changes_with_runtime_hash() {
        // A doge-runtime change (a different DOGE_RUNTIME_HASH) must invalidate
        // cached binaries even when the script source is byte-identical.
        let a = cache_key_from("0.1.1", "m6", "aaaaaaaaaaaaaaaa", "bark 1\nwow\n");
        let b = cache_key_from("0.1.1", "m6", "bbbbbbbbbbbbbbbb", "bark 1\nwow\n");
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_is_sixteen_hex_digits() {
        let key = cache_key("bark 1\nwow\n");
        assert_eq!(key.len(), 16);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
