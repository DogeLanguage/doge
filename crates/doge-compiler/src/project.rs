//! Turning a project's `doge.toml` into a [`DependencyMap`] the module loader can
//! resolve `so <alias>` imports against. Path dependencies are resolved from disk
//! here; git dependencies are handed to a caller-supplied `git_dir` closure (only
//! `dogelang` shells out to git and touches the cache — this crate stays free of
//! network and cache concerns). Resolution follows the dependency graph
//! transitively, keying each package by its canonical root directory so a file's
//! owning package (and thus which dependencies it can see) is unambiguous.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::diagnostics::{source_line, split_source_lines, Diagnostic};
use crate::manifest::{self, Dependency, DependencySource, GitRev, Manifest, MANIFEST_NAME};

/// Every package in a resolved project: canonical package-root directory → the
/// aliases that package declares → the canonical entry `.doge` file each binds.
/// The loader finds a file's owning package by walking its canonical ancestors to
/// the nearest directory present as a key here.
pub type DependencyMap = HashMap<PathBuf, HashMap<String, PathBuf>>;

const MANIFEST_HEADLINE: &str = "very manifest. much confuse.";
const RESOLVE_HEADLINE: &str = "very dependency. much missing.";

/// The nearest ancestor of `start` (inclusive) that holds a `doge.toml`, or
/// `None` when `start` is not inside any project.
pub fn discover_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(MANIFEST_NAME).is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// Read and parse the `doge.toml` at `root`.
pub fn read_manifest(root: &Path) -> Result<Manifest, Diagnostic> {
    let path = root.join(MANIFEST_NAME);
    let source = std::fs::read_to_string(&path).map_err(|err| {
        Diagnostic::new(
            path.to_string_lossy(),
            1,
            1,
            "",
            format!("doge could not read the manifest: {err}"),
        )
        .with_headline(MANIFEST_HEADLINE)
        .with_hint("every project has a doge.toml at its root")
    })?;
    manifest::parse(&path.to_string_lossy(), &source)
}

/// Resolve `root`'s dependency graph into a [`DependencyMap`]. `git_dir` turns a
/// git source into a local package directory (fetching or reading a cache); a
/// path dependency is resolved relative to the package declaring it.
pub fn resolve_project<F>(root: &Path, git_dir: &mut F) -> Result<DependencyMap, Diagnostic>
where
    F: FnMut(&str, &GitRev) -> Result<PathBuf, String>,
{
    let mut map = DependencyMap::new();
    let root_manifest = read_manifest(root)?;
    resolve_package(root, &root_manifest, &mut map, git_dir)?;
    Ok(map)
}

/// Resolve one package's dependencies into `map` and recurse into each. `manifest`
/// is the already-parsed manifest for `pkg`.
fn resolve_package<F>(
    pkg: &Path,
    manifest: &Manifest,
    map: &mut DependencyMap,
    git_dir: &mut F,
) -> Result<(), Diagnostic>
where
    F: FnMut(&str, &GitRev) -> Result<PathBuf, String>,
{
    let canon = std::fs::canonicalize(pkg).map_err(|err| {
        Diagnostic::new(
            pkg.to_string_lossy(),
            1,
            1,
            "",
            format!("doge could not open the project directory: {err}"),
        )
        .with_headline(RESOLVE_HEADLINE)
        .with_hint("check the project path")
    })?;
    // Reserve the slot before recursing so a dependency cycle terminates instead
    // of looping; the real entry map replaces it once the deps are resolved.
    map.insert(canon.clone(), HashMap::new());

    let manifest_path = pkg.join(MANIFEST_NAME).to_string_lossy().into_owned();
    let mut aliases = HashMap::new();

    for dep in &manifest.dependencies {
        let dep_dir = dep_directory(pkg, &manifest_path, dep, git_dir)?;
        let dep_manifest = read_dep_manifest(&dep_dir, &manifest_path, dep)?;
        let entry = dep_dir.join(&dep_manifest.entry);
        let entry_canon = std::fs::canonicalize(&entry).map_err(|_| {
            resolve_diag(
                &manifest_path,
                dep,
                &format!(
                    "the {} package has no entry file {}",
                    dep.alias, dep_manifest.entry
                ),
                "check the dependency's [package] entry",
            )
        })?;
        aliases.insert(dep.alias.clone(), entry_canon);

        if let Ok(dep_canon) = std::fs::canonicalize(&dep_dir) {
            if !map.contains_key(&dep_canon) {
                resolve_package(&dep_dir, &dep_manifest, map, git_dir)?;
            }
        }
    }

    map.insert(canon, aliases);
    Ok(())
}

/// The local directory for one dependency: a path source resolved against the
/// declaring package, or a git source handed to `git_dir`.
fn dep_directory<F>(
    pkg: &Path,
    manifest_path: &str,
    dep: &Dependency,
    git_dir: &mut F,
) -> Result<PathBuf, Diagnostic>
where
    F: FnMut(&str, &GitRev) -> Result<PathBuf, String>,
{
    match &dep.source {
        DependencySource::Path(rel) => Ok(pkg.join(rel)),
        DependencySource::Git { url, rev } => git_dir(url, rev).map_err(|message| {
            resolve_diag(
                manifest_path,
                dep,
                &message,
                "run doge bark to fetch dependencies",
            )
        }),
    }
}

/// Read a dependency package's own manifest, reporting the failure against the
/// line that declared it in the parent manifest.
fn read_dep_manifest(
    dep_dir: &Path,
    manifest_path: &str,
    dep: &Dependency,
) -> Result<Manifest, Diagnostic> {
    let path = dep_dir.join(MANIFEST_NAME);
    let source = std::fs::read_to_string(&path).map_err(|_| {
        resolve_diag(
            manifest_path,
            dep,
            &format!("doge found no package at {}", dep_dir.display()),
            "a dependency is a directory with its own doge.toml",
        )
    })?;
    manifest::parse(&path.to_string_lossy(), &source)
}

/// A dependency-resolution diagnostic anchored at the dependency's line in the
/// declaring `doge.toml`.
fn resolve_diag(manifest_path: &str, dep: &Dependency, message: &str, hint: &str) -> Diagnostic {
    let line = std::fs::read_to_string(manifest_path)
        .map(|source| source_line(&split_source_lines(&source), dep.line))
        .unwrap_or_default();
    Diagnostic::new(manifest_path, dep.line, 1, line, message)
        .with_headline(RESOLVE_HEADLINE)
        .with_hint(hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("doge-projtest-{tag}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create temp dir");
            TempDir(dir)
        }
        fn write(&self, name: &str, contents: &str) {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create fixture dir");
            }
            std::fs::write(&path, contents).expect("write fixture");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn canon(path: &Path) -> PathBuf {
        std::fs::canonicalize(path).expect("path should exist")
    }

    #[test]
    fn resolves_a_transitive_path_dependency_graph() {
        let dir = TempDir::new("path-graph");
        dir.write("util/doge.toml", "[package]\nname = \"util\"\n");
        dir.write(
            "util/main.doge",
            "such id much n:\n    return n\nwow\nwow\n",
        );
        dir.write(
            "greet/doge.toml",
            "[package]\nname = \"greet\"\n\n[dependencies]\nutil = { path = \"../util\" }\n",
        );
        dir.write(
            "greet/main.doge",
            "so util\nsuch hi:\n    return 1\nwow\nwow\n",
        );
        dir.write(
            "app/doge.toml",
            "[package]\nname = \"app\"\n\n[dependencies]\ngreet = { path = \"../greet\" }\n",
        );
        dir.write("app/main.doge", "so greet\nbark greet.hi()\nwow\n");

        let app = dir.0.join("app");
        let map = resolve_project(&app, &mut |_, _| unreachable!("no git deps here"))
            .expect("should resolve");

        assert_eq!(
            map[&canon(&app)]["greet"],
            canon(&dir.0.join("greet/main.doge"))
        );
        assert_eq!(
            map[&canon(&dir.0.join("greet"))]["util"],
            canon(&dir.0.join("util/main.doge"))
        );
        assert!(map[&canon(&dir.0.join("util"))].is_empty());
    }

    #[test]
    fn a_git_dependency_is_resolved_through_the_closure() {
        let dir = TempDir::new("git-closure");
        // The "fetched" git package, prepared on disk by the closure.
        dir.write("vendor/cool/doge.toml", "[package]\nname = \"cool\"\n");
        dir.write("vendor/cool/main.doge", "such f:\n    return 1\nwow\nwow\n");
        dir.write(
            "app/doge.toml",
            "[package]\nname = \"app\"\n\n[dependencies]\ncool = { git = \"https://example.com/cool.git\", tag = \"v1\" }\n",
        );
        dir.write("app/main.doge", "so cool\nbark cool.f()\nwow\n");

        let vendor = dir.0.join("vendor/cool");
        let mut calls = 0;
        let map = resolve_project(&dir.0.join("app"), &mut |url, rev| {
            calls += 1;
            assert_eq!(url, "https://example.com/cool.git");
            assert_eq!(rev.as_ref_name(), Some("v1"));
            Ok(vendor.clone())
        })
        .expect("should resolve");

        assert_eq!(calls, 1, "the git closure runs once for the git dep");
        assert_eq!(
            map[&canon(&dir.0.join("app"))]["cool"],
            canon(&vendor.join("main.doge"))
        );
    }

    #[test]
    fn a_missing_path_dependency_is_reported() {
        let dir = TempDir::new("path-missing");
        dir.write(
            "app/doge.toml",
            "[package]\nname = \"app\"\n\n[dependencies]\ngone = { path = \"../gone\" }\n",
        );
        dir.write("app/main.doge", "so gone\nwow\n");

        let err = resolve_project(&dir.0.join("app"), &mut |_, _| unreachable!())
            .expect_err("a missing path dep should fail");
        assert_eq!(err.headline, RESOLVE_HEADLINE);
        assert!(err.message.contains("no package"));
    }
}
