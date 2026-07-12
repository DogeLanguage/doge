//! Resolving a project's dependencies for the CLI: discover the `doge.toml` above
//! a script, fetch any git dependencies into the cache, and hand the compiler a
//! resolved [`DependencyMap`]. Path dependencies are resolved by the compiler
//! itself; this module supplies the one thing the compiler crate deliberately
//! leaves out — shelling out to `git` and caching the result, pinned through
//! `doge.lock` for reproducibility.

use std::path::{Path, PathBuf};
use std::process::Command;

use doge_compiler::{DependencyMap, GitRev};

use crate::cache;

/// The lockfile that pins git dependencies, at the project root.
const LOCK_NAME: &str = "doge.lock";

const GIT_MISSING: &str = "\
very git. much missing.

  a git dependency needs git, but it wasn't found.

such fix: install git from https://git-scm.com";

const NO_PROJECT: &str = "\
very project. much missing.

  doge found no doge.toml here or in any parent directory.

such fix: run doge new <name>, or pass a script path";

/// A located entry point plus its resolved project context.
pub struct Located {
    /// The script to compile and run.
    pub entry: PathBuf,
    /// The resolved dependency graph (empty for a bare script with no project).
    pub deps: DependencyMap,
    /// The project's package name, when the entry belongs to a project.
    pub package_name: Option<String>,
}

/// Find the entry script and resolve its project. With an explicit `path`, that
/// script is the entry and any enclosing `doge.toml` supplies its dependencies;
/// without one, the nearest project's `[package].entry` is used.
pub fn locate(explicit: Option<&str>) -> Result<Located, String> {
    match explicit {
        Some(path) => {
            let entry = PathBuf::from(path);
            let start = entry
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            match doge_compiler::discover_root(&start) {
                Some(root) => resolve_at(&root, entry),
                None => Ok(Located {
                    entry,
                    deps: DependencyMap::new(),
                    package_name: None,
                }),
            }
        }
        None => {
            let cwd = std::env::current_dir()
                .map_err(|err| format!("very lost. much confuse.\n\n  doge could not read the current directory: {err}"))?;
            let root = doge_compiler::discover_root(&cwd).ok_or_else(|| NO_PROJECT.to_string())?;
            let manifest = doge_compiler::read_manifest(&root).map_err(|diag| diag.render())?;
            let entry = root.join(&manifest.entry);
            resolve_at(&root, entry)
        }
    }
}

/// Resolve the project at `root`, returning the located entry with its dependency
/// graph and package name.
fn resolve_at(root: &Path, entry: PathBuf) -> Result<Located, String> {
    let manifest = doge_compiler::read_manifest(root).map_err(|diag| diag.render())?;
    let mut lock = Lock::load(root);
    let deps = {
        let deps_dir = cache::deps_root()?;
        let mut git = |url: &str, rev: &GitRev| ensure_git(&deps_dir, &mut lock, url, rev);
        doge_compiler::resolve_project(root, &mut git).map_err(|diag| diag.render())?
    };
    lock.save(root)?;
    Ok(Located {
        entry,
        deps,
        package_name: Some(manifest.name),
    })
}

/// Ensure a git dependency is present locally and return its package directory.
/// A locked, already-cached checkout is reused offline; otherwise the repository
/// is cloned, checked out at the requested revision, and pinned to the resolved
/// commit under `<cache>/deps/git/<url-hash>/<sha>/`.
fn ensure_git(
    deps_dir: &Path,
    lock: &mut Lock,
    url: &str,
    rev: &GitRev,
) -> Result<PathBuf, String> {
    let spec = rev.as_ref_name().unwrap_or("default");
    let base = deps_dir.join("git").join(url_hash(url));

    if let Some(sha) = lock.get(url, spec) {
        let pinned = base.join(&sha);
        if pinned.join(doge_compiler::MANIFEST_NAME).is_file() {
            return Ok(pinned);
        }
    }

    detect_git()?;
    std::fs::create_dir_all(&base).map_err(disk_err)?;
    let staging = base.join(format!("staging-{}", url_hash(spec)));
    let _ = std::fs::remove_dir_all(&staging);
    git_clone(url, &staging)?;
    if let Some(reference) = rev.as_ref_name() {
        git_checkout(&staging, reference)?;
    }
    let sha = git_head(&staging)?;

    let pinned = base.join(&sha);
    if pinned.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    } else {
        std::fs::rename(&staging, &pinned).map_err(disk_err)?;
    }
    lock.set(url, spec, &sha);
    Ok(pinned)
}

fn detect_git() -> Result<(), String> {
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(GIT_MISSING.to_string()),
    }
}

fn git_clone(url: &str, dest: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["clone", "--quiet", url])
        .arg(dest)
        .output()
        .map_err(|_| GIT_MISSING.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_error("clone", url, &output))
    }
}

fn git_checkout(dir: &Path, reference: &str) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["checkout", "--quiet", reference])
        .output()
        .map_err(|_| GIT_MISSING.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_error(
            &format!("checkout {reference} of"),
            "the dependency",
            &output,
        ))
    }
}

fn git_head(dir: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|_| GIT_MISSING.to_string())?;
    if !output.status.success() {
        return Err(git_error("read the commit of", "the dependency", &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A git failure, framed as a dependency problem (not a doge bug — the repository,
/// revision, or network is the user's to fix). Git's own message is included.
fn git_error(action: &str, subject: &str, output: &std::process::Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.trim();
    format!(
        "very git. much fail.\n\n  doge could not {action} {subject}: {detail}\n\nsuch fix: check the git url, revision, and your network"
    )
}

fn url_hash(value: &str) -> String {
    format!("{:016x}", cache::fnv1a_64(value.as_bytes()))
}

fn disk_err(err: std::io::Error) -> String {
    format!("very disk. much sad.\n\n  doge could not write its dependency cache: {err}")
}

/// The `doge.lock` file: pinned resolved commits for git dependencies, keyed by
/// `(url, requested-spec)`. It is a cache for reproducibility — a corrupt or
/// missing lock is simply treated as empty and regenerated.
struct Lock {
    entries: Vec<LockEntry>,
}

struct LockEntry {
    url: String,
    spec: String,
    resolved: String,
}

impl Lock {
    fn load(root: &Path) -> Lock {
        let source = std::fs::read_to_string(root.join(LOCK_NAME)).unwrap_or_default();
        Lock {
            entries: parse_lock(&source),
        }
    }

    fn get(&self, url: &str, spec: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|e| e.url == url && e.spec == spec)
            .map(|e| e.resolved.clone())
    }

    fn set(&mut self, url: &str, spec: &str, resolved: &str) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.url == url && e.spec == spec)
        {
            entry.resolved = resolved.to_string();
        } else {
            self.entries.push(LockEntry {
                url: url.to_string(),
                spec: spec.to_string(),
                resolved: resolved.to_string(),
            });
        }
    }

    fn save(&self, root: &Path) -> Result<(), String> {
        if self.entries.is_empty() {
            return Ok(());
        }
        let mut sorted: Vec<&LockEntry> = self.entries.iter().collect();
        sorted.sort_by(|a, b| (&a.url, &a.spec).cmp(&(&b.url, &b.spec)));

        let mut out = String::from("# doge.lock — generated by doge, do not edit\n");
        for entry in sorted {
            out.push_str(&format!(
                "\n[[git]]\nurl = \"{}\"\nrev = \"{}\"\nresolved = \"{}\"\n",
                entry.url, entry.spec, entry.resolved
            ));
        }
        std::fs::write(root.join(LOCK_NAME), out).map_err(disk_err)
    }
}

/// Parse a `doge.lock`: `[[git]]` blocks of `url`/`rev`/`resolved` string pairs.
/// Anything unrecognized is ignored, so an old or hand-edited lock never errors.
fn parse_lock(source: &str) -> Vec<LockEntry> {
    let mut entries = Vec::new();
    let mut url = None;
    let mut spec = None;
    let mut resolved = None;
    let mut flush =
        |url: &mut Option<String>, spec: &mut Option<String>, resolved: &mut Option<String>| {
            if let (Some(u), Some(s), Some(r)) = (url.take(), spec.take(), resolved.take()) {
                entries.push(LockEntry {
                    url: u,
                    spec: s,
                    resolved: r,
                });
            }
        };

    for line in source.lines() {
        let line = line.trim();
        if line == "[[git]]" {
            flush(&mut url, &mut spec, &mut resolved);
        } else if let Some(value) = lock_value(line, "url") {
            url = Some(value);
        } else if let Some(value) = lock_value(line, "rev") {
            spec = Some(value);
        } else if let Some(value) = lock_value(line, "resolved") {
            resolved = Some(value);
        }
    }
    flush(&mut url, &mut spec, &mut resolved);
    entries
}

/// The quoted value of a `key = "value"` lock line, if `line` is that key.
fn lock_value(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim();
    let inner = rest.strip_prefix('"')?.strip_suffix('"')?;
    Some(inner.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_round_trips() {
        let source =
            "# doge.lock\n\n[[git]]\nurl = \"https://example.com/a.git\"\nrev = \"v1\"\nresolved = \"abc123\"\n";
        let mut lock = Lock {
            entries: parse_lock(source),
        };
        assert_eq!(
            lock.get("https://example.com/a.git", "v1").as_deref(),
            Some("abc123")
        );
        assert_eq!(lock.get("https://example.com/a.git", "v2"), None);

        lock.set("https://example.com/b.git", "default", "def456");
        assert_eq!(
            lock.get("https://example.com/b.git", "default").as_deref(),
            Some("def456")
        );
    }

    #[test]
    fn setting_an_existing_key_updates_it() {
        let mut lock = Lock {
            entries: Vec::new(),
        };
        lock.set("u", "s", "one");
        lock.set("u", "s", "two");
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(lock.get("u", "s").as_deref(), Some("two"));
    }
}
