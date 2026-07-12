//! The module loader: it turns an entry `.doge` script into a whole [`Program`]
//! by following `so` imports. A `so <name>` first resolves against the stdlib
//! table ([`crate::stdlib`]); anything else is a user module — the file
//! `<name>.doge` next to the importing file. Loading is recursive (a module may
//! import other modules) with cycle detection, so the pipeline downstream of
//! here — checks and codegen — sees every file at once.

use std::collections::HashMap;
use std::path::Path;

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
        by_name: HashMap::new(),
        init_order: Vec::new(),
        active: Vec::new(),
        next_id: 1,
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
        if let Stmt::Import { module, span } = stmt {
            match stdlib::module(module) {
                Some(m) => stdlib_imports.push((module.clone(), m)),
                None => return Err(unknown_stdlib_module(path, source, module, *span)),
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
    /// Module name → file id, so a module imported twice loads once.
    by_name: HashMap<String, u32>,
    /// Module file ids in dependency order (completion order).
    init_order: Vec<u32>,
    /// Module names on the current DFS path, for cycle detection.
    active: Vec<String>,
    /// Next module file id to hand out (the entry is always 0).
    next_id: u32,
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
            let Stmt::Import { module, span } = stmt else {
                continue;
            };

            if let Some(entry) = stdlib::module(module) {
                if module_file_exists(dir, module) {
                    return Err(shadow_diag(importer_path, importer_source, module, *span));
                }
                stdlib_imports.push((module.clone(), entry));
                continue;
            }

            if self.active.iter().any(|m| m == module) {
                return Err(cycle_diag(
                    importer_path,
                    importer_source,
                    &self.active,
                    module,
                    *span,
                ));
            }

            let target_id = match self.by_name.get(module) {
                Some(id) => *id,
                None => self.load_module(dir, importer_path, importer_source, module, *span)?,
            };
            user_imports.push((module.clone(), target_id));
        }

        Ok((stdlib_imports, user_imports))
    }

    /// Read, parse, and recursively resolve a user module `<name>.doge` sitting
    /// next to its importer. Returns the new file id.
    fn load_module(
        &mut self,
        dir: Option<&Path>,
        importer_path: &str,
        importer_source: &str,
        name: &str,
        span: Span,
    ) -> Result<u32, Diagnostic> {
        let path = module_path(dir, name);
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(missing_module_diag(
                    importer_path,
                    importer_source,
                    name,
                    span,
                ))
            }
            Err(err) => {
                return Err(read_error_diag(
                    importer_path,
                    importer_source,
                    name,
                    &path,
                    &err,
                    span,
                ))
            }
        };

        let path_str = path.to_string_lossy().into_owned();
        let script = crate::parser::parse(&path_str, &source)?;

        let file_id = self.next_id;
        self.next_id += 1;
        self.by_name.insert(name.to_string(), file_id);

        self.active.push(name.to_string());
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

fn module_path(dir: Option<&Path>, name: &str) -> std::path::PathBuf {
    let file = format!("{name}.doge");
    match dir {
        Some(dir) => dir.join(file),
        None => std::path::PathBuf::from(file),
    }
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
