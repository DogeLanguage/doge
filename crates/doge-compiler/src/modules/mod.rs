//! The module loader: it turns an entry `.doge` script into a whole [`Program`]
//! by following `so` imports. A bare `so <name>` first resolves against the
//! stdlib table ([`crate::stdlib`]); anything else is a user module — the file
//! `<name>.doge` next to the importing file. A string-path import
//! `so "sub/dir/mod.doge"` names a user file by a `/`-separated path relative to
//! the importing file, binding the file's stem. Loading is recursive (a module
//! may import other modules); dedup and cycle detection key on each file's
//! canonical path, so the same file reached by two routes loads once and two
//! different files may share a stem. The pipeline downstream of here — checks and
//! codegen — sees every file at once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(super) use crate::ast::{Script, Stmt};
pub(super) use crate::diagnostics::Diagnostic;
pub(super) use crate::stdlib::{self, Module};
pub(super) use crate::token::Span;

mod diag;
use diag::*;

#[cfg(test)]
mod tests;

pub struct Program {
    pub files: Vec<ProgramFile>,
    /// Module file ids in dependency order (a module before anything that
    /// imports it), so their constants can be initialized in a valid order. The
    /// entry is not listed — its constants initialize inline with its own
    /// top-level statements.
    pub init_order: Vec<u32>,
}

/// One source file in the program, with its parsed script and resolved imports.
pub struct ProgramFile {
    pub file_id: u32,
    pub is_entry: bool,
    /// The module name (a user module's import name, or the entry's file stem).
    pub name: String,
    pub path: String,
    pub source: String,
    pub script: Script,
    /// Imported stdlib modules: `(local name, table entry)`.
    pub stdlib_imports: Vec<(String, &'static Module)>,
    /// Imported user modules: `(local name, target file id)`.
    pub user_imports: Vec<(String, u32)>,
}

/// Load the entry script and every module it imports into a [`Program`].
pub fn load_program(entry_path: &str, entry_source: &str) -> Result<Program, Diagnostic> {
    let entry_script = crate::parser::parse(entry_path, entry_source)?;
    let mut loader = Loader {
        modules: Vec::new(),
        by_path: HashMap::new(),
        init_order: Vec::new(),
        active: Vec::new(),
        next_id: 1,
        entry_key: canonical_key(Path::new(entry_path)),
    };
    let (stdlib_imports, user_imports) =
        loader.resolve_imports(entry_path, entry_source, &entry_script)?;

    let entry = ProgramFile {
        file_id: 0,
        is_entry: true,
        name: file_stem(entry_path),
        path: entry_path.to_string(),
        source: entry_source.to_string(),
        script: entry_script,
        stdlib_imports,
        user_imports,
    };

    // `modules` is in completion (dependency) order with ids assigned in
    // discovery order; place each at its file_id so `files[i].file_id == i`.
    let init_order = loader.modules.iter().map(|m| m.file_id).collect();
    let mut slots: Vec<Option<ProgramFile>> = (0..=loader.modules.len()).map(|_| None).collect();
    slots[0] = Some(entry);
    for module in loader.modules {
        let id = module.file_id as usize;
        slots[id] = Some(module);
    }
    let files = slots
        .into_iter()
        .map(|s| s.expect("compiler bug: unfilled program file slot"))
        .collect();

    Ok(Program { files, init_order })
}

/// Build a one-file [`Program`] from an already-parsed script, resolving imports
/// against the stdlib only. Used by the single-file `generate`/`check` APIs and
/// the codegen unit tests, where no user modules are on disk to load.
pub fn single_file_program(
    path: &str,
    source: &str,
    script: Script,
) -> Result<Program, Diagnostic> {
    let mut stdlib_imports = Vec::new();
    for stmt in &script.stmts {
        if let Stmt::Import {
            module,
            path: import_path,
            span,
        } = stmt
        {
            // Single-file mode has no loader, so a user module — bare or path —
            // cannot be resolved here; only stdlib imports are valid.
            match (import_path, stdlib::module(module)) {
                (None, Some(m)) => stdlib_imports.push((module.clone(), m)),
                _ => return Err(unknown_stdlib_module(path, source, module, *span)),
            }
        }
    }
    let entry = ProgramFile {
        file_id: 0,
        is_entry: true,
        name: file_stem(path),
        path: path.to_string(),
        source: source.to_string(),
        script,
        stdlib_imports,
        user_imports: Vec::new(),
    };
    Ok(Program {
        files: vec![entry],
        init_order: Vec::new(),
    })
}

/// A file's resolved imports: `(stdlib name → entry, user name → file id)`.
type ResolvedImports = (Vec<(String, &'static Module)>, Vec<(String, u32)>);

struct Loader {
    /// Loaded modules in completion (dependency) order.
    modules: Vec<ProgramFile>,
    /// Canonical module path → file id, so a file reached twice loads once even
    /// when the two routes spell its path differently.
    by_path: HashMap<PathBuf, u32>,
    /// Module file ids in dependency order (completion order).
    init_order: Vec<u32>,
    /// Modules on the current DFS path, for cycle detection.
    active: Vec<ActiveModule>,
    /// Next module file id to hand out (the entry is always 0).
    next_id: u32,
    /// The entry file's canonical path, so a module importing the entry back is
    /// caught (the entry is not a module — it has loose top-level statements).
    entry_key: PathBuf,
}

/// A module currently being loaded on the DFS path: its binding name (for a
/// readable cycle message) and its canonical path (the identity we match on).
struct ActiveModule {
    name: String,
    key: PathBuf,
}

impl Loader {
    /// Resolve one file's `so` imports, loading any user modules it names, and
    /// return its `(stdlib imports, user imports)`.
    fn resolve_imports(
        &mut self,
        importer_path: &str,
        importer_source: &str,
        script: &Script,
    ) -> Result<ResolvedImports, Diagnostic> {
        let dir = Path::new(importer_path).parent();
        let mut stdlib_imports = Vec::new();
        let mut user_imports = Vec::new();

        for stmt in &script.stmts {
            let Stmt::Import { module, path, span } = stmt else {
                continue;
            };

            // A bare `so name` may resolve to a stdlib module; a string-path
            // import always names a user file, so its stem must not collide with
            // a stdlib module (the binding would be unusable).
            match path {
                None => {
                    if let Some(entry) = stdlib::module(module) {
                        if module_file_exists(dir, module) {
                            return Err(shadow_diag(importer_path, importer_source, module, *span));
                        }
                        stdlib_imports.push((module.clone(), entry));
                        continue;
                    }
                }
                Some(_) if stdlib::module(module).is_some() => {
                    return Err(shadow_diag(importer_path, importer_source, module, *span));
                }
                Some(_) => {}
            }

            let target = match path {
                Some(raw) => path_import_path(dir, raw),
                None => module_path(dir, module),
            };
            let target_id = self.resolve_user_module(
                importer_path,
                importer_source,
                module,
                path.as_deref(),
                &target,
                *span,
            )?;
            user_imports.push((module.clone(), target_id));
        }

        Ok((stdlib_imports, user_imports))
    }

    /// Resolve one user-module import to a file id: canonicalize its path (which
    /// also detects a missing file), reject a self-import of the entry, check the
    /// active DFS path for a cycle, reuse an already-loaded file, or load it.
    fn resolve_user_module(
        &mut self,
        importer_path: &str,
        importer_source: &str,
        name: &str,
        raw_path: Option<&str>,
        target: &Path,
        span: Span,
    ) -> Result<u32, Diagnostic> {
        let key = match std::fs::canonicalize(target) {
            Ok(key) => key,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(match raw_path {
                    Some(raw) => {
                        missing_path_module_diag(importer_path, importer_source, raw, target, span)
                    }
                    None => missing_module_diag(importer_path, importer_source, name, span),
                });
            }
            Err(err) => {
                return Err(read_error_diag(
                    importer_path,
                    importer_source,
                    name,
                    target,
                    &err,
                    span,
                ))
            }
        };

        if key == self.entry_key {
            return Err(entry_import_diag(importer_path, importer_source, span));
        }

        if self.active.iter().any(|m| m.key == key) {
            let chain: Vec<String> = self.active.iter().map(|m| m.name.clone()).collect();
            return Err(cycle_diag(
                importer_path,
                importer_source,
                &chain,
                name,
                span,
            ));
        }

        if let Some(id) = self.by_path.get(&key) {
            return Ok(*id);
        }

        self.load_module(name, target, key, importer_path, importer_source, span)
    }

    /// Read, parse, and recursively resolve a user module at `target` (already
    /// canonicalized to `key`). Returns the new file id.
    fn load_module(
        &mut self,
        name: &str,
        target: &Path,
        key: PathBuf,
        importer_path: &str,
        importer_source: &str,
        span: Span,
    ) -> Result<u32, Diagnostic> {
        let source = match std::fs::read_to_string(target) {
            Ok(source) => source,
            Err(err) => {
                return Err(read_error_diag(
                    importer_path,
                    importer_source,
                    name,
                    target,
                    &err,
                    span,
                ))
            }
        };

        let path_str = target.to_string_lossy().into_owned();
        let script = crate::parser::parse(&path_str, &source)?;

        let file_id = self.next_id;
        self.next_id += 1;
        self.by_path.insert(key.clone(), file_id);

        self.active.push(ActiveModule {
            name: name.to_string(),
            key,
        });
        let (stdlib_imports, user_imports) = self.resolve_imports(&path_str, &source, &script)?;
        self.active.pop();

        self.modules.push(ProgramFile {
            file_id,
            is_entry: false,
            name: name.to_string(),
            path: path_str,
            source,
            script,
            stdlib_imports,
            user_imports,
        });
        self.init_order.push(file_id);
        Ok(file_id)
    }
}

fn module_path(dir: Option<&Path>, name: &str) -> PathBuf {
    let file = format!("{name}.doge");
    match dir {
        Some(dir) => dir.join(file),
        None => PathBuf::from(file),
    }
}

/// The file a string-path import `so "raw"` targets, relative to the importing
/// file's directory. `raw` is validated by the parser (relative, `/`-separated).
fn path_import_path(dir: Option<&Path>, raw: &str) -> PathBuf {
    match dir {
        Some(dir) => dir.join(raw),
        None => PathBuf::from(raw),
    }
}

/// A file's identity for dedup and cycle detection: its canonical path when it
/// resolves, else the path as given (so identity is still stable enough to work
/// with before the file is confirmed to exist).
fn canonical_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn module_file_exists(dir: Option<&Path>, name: &str) -> bool {
    module_path(dir, name).is_file()
}

fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}
