//! Salt the build cache with the `doge-runtime` source, so a runtime change
//! always invalidates cached script binaries. We hash every `.rs` file in the
//! runtime crate plus its `Cargo.toml` and expose the digest as `DOGE_RUNTIME_HASH`,
//! which `cache.rs` folds into each script's cache key. The `rerun-if-changed`
//! lines make cargo rebuild this script (and the digest) whenever the runtime
//! source changes.

use std::path::{Path, PathBuf};

/// FNV-1a 64-bit offset basis and prime (the canonical constants) — kept in sync
/// with `cache.rs`, which uses the same hash for cache keys.
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let runtime = Path::new(&manifest).join("../doge-runtime");

    let mut files = Vec::new();
    collect_rs_files(&runtime.join("src"), &mut files);
    files.push(runtime.join("Cargo.toml"));
    // Sort so the digest is independent of directory-read order.
    files.sort();

    let mut hash = FNV_OFFSET;
    for file in &files {
        println!("cargo:rerun-if-changed={}", file.display());
        if let Ok(bytes) = std::fs::read(file) {
            for byte in bytes {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
    }

    println!("cargo:rustc-env=DOGE_RUNTIME_HASH={hash:016x}");
}

/// Every `.rs` file under `dir`, recursively.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}
