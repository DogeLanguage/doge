//! Turning generated Rust into a runnable native binary. Each
//! script gets its own tiny Cargo project under the cache, all sharing one
//! target dir so `doge-runtime` compiles once.
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cache::{self, CachePaths};

const RUST_MISSING: &str = "\
very rust. much missing.

  doge compiles through Rust's toolchain, but cargo wasn't found.

such fix: install Rust from https://rustup.rs";

/// Compile `source` to a cached native binary and return its path, reusing the
/// cache when the script is unchanged. Every error is a fully rendered,
/// doge-flavored message ready to print to stderr.
pub fn ensure_binary(source: &str, generated: &str) -> Result<PathBuf, String> {
    let paths = cache::resolve(source)?;
    if cache::cache_hit(&paths.entry_dir, &paths.binary, source) {
        return Ok(paths.binary);
    }
    detect_toolchain()?;
    write_entry(&paths, source, generated)?;
    compile(&paths)?;
    Ok(paths.binary)
}

/// Run a freshly built binary with inherited stdio and return its exit code, so
/// `doge bark` exits exactly as the script did.
pub fn spawn(binary: &Path) -> Result<i32, String> {
    match Command::new(binary).status() {
        Ok(status) => Ok(status.code().unwrap_or(1)),
        Err(err) => Err(format!(
            "very run. much fail.\n\n  doge built your script but could not run it: {err}"
        )),
    }
}

/// Copy the cached binary next to the user, as `./<stem>`.
pub fn copy_to_cwd(binary: &Path, stem: &str) -> Result<(), String> {
    let dest = PathBuf::from(format!("./{stem}"));
    std::fs::copy(binary, &dest)
        .map(|_| ())
        .map_err(|err| format!("very disk. much sad.\n\n  doge could not write ./{stem}: {err}"))
}

fn detect_toolchain() -> Result<(), String> {
    match Command::new("cargo").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(RUST_MISSING.to_string()),
    }
}

/// The `doge-runtime` crate, as an absolute path. M3 pins the build to this dev
/// checkout; the packaging story (a bundled runtime) is M6.
fn runtime_path() -> Result<PathBuf, String> {
    let raw = concat!(env!("CARGO_MANIFEST_DIR"), "/../doge-runtime");
    std::fs::canonicalize(raw).map_err(|err| {
        format!("very rust. much missing.\n\n  doge cannot find its runtime crate at {raw}: {err}")
    })
}

fn write_entry(paths: &CachePaths, source: &str, generated: &str) -> Result<(), String> {
    let runtime = runtime_path()?;
    let src_dir = paths.entry_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(disk_err)?;

    let runtime_lit = format!("{:?}", runtime.to_string_lossy());
    // The empty `[workspace]` makes this its own workspace root, so it never
    // attaches to a repo workspace when the cache happens to live inside one.
    let cargo_toml = format!(
        "[package]\n\
         name = \"{pkg}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2021\"\n\
         \n\
         [workspace]\n\
         \n\
         [dependencies]\n\
         doge-runtime = {{ path = {runtime_lit} }}\n\
         \n\
         [[bin]]\n\
         name = \"{pkg}\"\n\
         path = \"src/main.rs\"\n",
        pkg = paths.package,
    );
    std::fs::write(paths.entry_dir.join("Cargo.toml"), cargo_toml).map_err(disk_err)?;
    std::fs::write(src_dir.join("main.rs"), generated).map_err(disk_err)?;
    // Written LAST: source.doge is the validity marker `cache_hit` checks, so it
    // must not exist until Cargo.toml and main.rs are safely on disk.
    std::fs::write(paths.entry_dir.join("source.doge"), source).map_err(disk_err)?;
    Ok(())
}

fn compile(paths: &CachePaths) -> Result<(), String> {
    let output = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--quiet")
        .current_dir(&paths.entry_dir)
        .env("CARGO_TARGET_DIR", &paths.target_dir)
        .output()
        .map_err(|_| RUST_MISSING.to_string())?;
    if output.status.success() {
        return Ok(());
    }

    // A rustc rejection of generated code is a doge bug (Hard Rule 11): capture
    // the output into build.log and report it as one — never spill it on screen.
    let log = paths.entry_dir.join("build.log");
    let mut captured = output.stdout;
    captured.extend_from_slice(&output.stderr);
    let _ = std::fs::write(&log, &captured);
    Err(format!(
        "very bug. much sorry.\n\n\
         the Rust that doge generated failed to build — this is a doge bug, not your script.\n\
         pls report it and attach: {}",
        log.display()
    ))
}

fn disk_err(err: std::io::Error) -> String {
    format!("very disk. much sad.\n\n  doge could not write its build cache: {err}")
}
