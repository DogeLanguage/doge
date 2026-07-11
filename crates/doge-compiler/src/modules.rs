//! The module loader: it turns an entry `.doge` script into a whole [`Program`]
//! by following `so` imports. A `so <name>` first resolves against the stdlib
//! table ([`crate::stdlib`]); anything else is a user module — the file
//! `<name>.doge` next to the importing file. Loading is recursive (a module may
//! import other modules) with cycle detection, so the pipeline downstream of
//! here — checks and codegen — sees every file at once.

use std::collections::HashMap;
use std::path::Path;

use crate::ast::{Script, Stmt};
use crate::diagnostics::Diagnostic;
use crate::stdlib::{self, Module};
use crate::token::Span;

/// The whole program: the entry file plus every module it transitively imports.
/// `files[i].file_id == i`; `files[0]` is always the entry.
pub struct Program {
    pub files: Vec<ProgramFile>,
    /// Module file ids in dependency order (a module before anything that
    /// imports it), so their constants can be initialized in a valid order. The
    /// entry is not listed — its constants initialize inline with its own
    /// top-level statements.
    pub(crate) init_order: Vec<u32>,
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
    pub(crate) stdlib_imports: Vec<(String, &'static Module)>,
    /// Imported user modules: `(local name, target file id)`.
    pub(crate) user_imports: Vec<(String, u32)>,
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

/// The source line a span points at, for building a diagnostic against a file's
/// text (mirrors the `lines` handling in `check`/`codegen`).
fn source_line(source: &str, line: u32) -> String {
    source
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .nth((line as usize).saturating_sub(1))
        .unwrap_or_default()
        .to_string()
}

fn diag(path: &str, source: &str, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        path,
        span.line,
        span.col,
        source_line(source, span.line),
        message,
    )
}

/// The "doge has no module named X" diagnostic for the stdlib-only path (used by
/// `single_file_program`), nudging `math` toward `nerd`.
pub(crate) fn unknown_stdlib_module(
    path: &str,
    source: &str,
    name: &str,
    span: Span,
) -> Diagnostic {
    let hint = if name == "math" {
        "much math? such nerd — write so nerd".to_string()
    } else {
        format!("modules: {}", stdlib::module_names())
    };
    diag(
        path,
        source,
        span,
        format!("doge has no module named {name}"),
    )
    .with_headline("very import. much unknown.")
    .with_hint(hint)
}

/// The "no such module, and no file for it either" diagnostic for a user import
/// whose `<name>.doge` is not next to the importing file.
fn missing_module_diag(path: &str, source: &str, name: &str, span: Span) -> Diagnostic {
    let hint = if name == "math" {
        "much math? such nerd — write so nerd".to_string()
    } else {
        format!(
            "make {name}.doge next to this file, or import a stdlib module ({})",
            stdlib::module_names()
        )
    };
    diag(
        path,
        source,
        span,
        format!("doge has no module named {name}"),
    )
    .with_headline("very import. much unknown.")
    .with_hint(hint)
}

fn read_error_diag(
    path: &str,
    source: &str,
    name: &str,
    target: &Path,
    err: &std::io::Error,
    span: Span,
) -> Diagnostic {
    diag(
        path,
        source,
        span,
        format!("doge found {name}.doge but could not read it: {err}"),
    )
    .with_headline("very import. much unreadable.")
    .with_hint(format!("check the file at {}", target.display()))
}

/// A user file whose name collides with a stdlib module: the import would always
/// mean the stdlib, so the file can never be reached — a name to fix now.
fn shadow_diag(path: &str, source: &str, name: &str, span: Span) -> Diagnostic {
    diag(
        path,
        source,
        span,
        format!("{name}.doge shadows the built-in module {name}"),
    )
    .with_headline("very shadow. much confuse.")
    .with_hint(format!("rename your file — {name} is a doge stdlib module"))
}

/// A circular import: `active` is the chain of modules currently being loaded,
/// and `name` closes the loop back onto one of them.
fn cycle_diag(path: &str, source: &str, active: &[String], name: &str, span: Span) -> Diagnostic {
    let start = active.iter().position(|m| m == name).unwrap_or(0);
    let mut chain: Vec<&str> = active[start..].iter().map(String::as_str).collect();
    chain.push(name);
    diag(
        path,
        source,
        span,
        format!("import cycle: {}", chain.join(" → ")),
    )
    .with_headline("very loop. much import.")
    .with_hint("break the loop — one of these imports has to go")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway directory under the system temp dir, cleaned up on drop.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("doge-modtest-{tag}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create temp dir");
            TempDir(dir)
        }
        fn write(&self, name: &str, contents: &str) -> String {
            let path = self.0.join(name);
            std::fs::write(&path, contents).expect("write fixture");
            path.to_string_lossy().into_owned()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn loads_transitive_modules_deps_first() {
        let dir = TempDir::new("transitive");
        dir.write(
            "helpers.doge",
            "such double much n:\n    return n + n\nwow\nwow\n",
        );
        dir.write(
            "utils.doge",
            "so helpers\nsuch quad much n:\n    return helpers.double(helpers.double(n))\nwow\nwow\n",
        );
        let entry = dir.write("app.doge", "so utils\nbark utils.quad(1)\nwow\n");
        let source = std::fs::read_to_string(&entry).unwrap();

        let program = match load_program(&entry, &source) {
            Ok(program) => program,
            Err(e) => panic!("should load: {}", e.render()),
        };
        assert_eq!(program.files.len(), 3, "entry + two modules");
        assert!(program.files[0].is_entry);
        assert_eq!(program.files[0].file_id, 0);
        // Constants initialize deps-first: helpers before utils.
        let names: Vec<&str> = program
            .init_order
            .iter()
            .map(|id| program.files[*id as usize].name.as_str())
            .collect();
        assert_eq!(names, vec!["helpers", "utils"]);
    }

    #[test]
    fn a_shared_module_loads_once() {
        let dir = TempDir::new("shared");
        dir.write("shared.doge", "so K = 1\nwow\n");
        dir.write(
            "a.doge",
            "so shared\nsuch fa:\n    return shared.K\nwow\nwow\n",
        );
        dir.write(
            "b.doge",
            "so shared\nsuch fb:\n    return shared.K\nwow\nwow\n",
        );
        let entry = dir.write("app.doge", "so a\nso b\nbark a.fa()\nwow\n");
        let source = std::fs::read_to_string(&entry).unwrap();

        let program = match load_program(&entry, &source) {
            Ok(program) => program,
            Err(e) => panic!("should load: {}", e.render()),
        };
        // entry + a + b + shared, with shared loaded a single time.
        assert_eq!(program.files.len(), 4);
    }

    #[test]
    fn a_cycle_is_reported() {
        let dir = TempDir::new("cycle");
        dir.write("a.doge", "so b\nsuch fa:\n    return 1\nwow\nwow\n");
        dir.write("b.doge", "so a\nsuch fb:\n    return 1\nwow\nwow\n");
        let entry = dir.write("app.doge", "so a\nbark a.fa()\nwow\n");
        let source = std::fs::read_to_string(&entry).unwrap();

        let err = match load_program(&entry, &source) {
            Err(e) => e,
            Ok(_) => panic!("a cycle should fail"),
        };
        assert_eq!(err.headline, "very loop. much import.");
        assert!(err.message.contains("import cycle"));
    }

    #[test]
    fn a_missing_user_module_is_unknown() {
        let dir = TempDir::new("missing");
        let entry = dir.write("app.doge", "so nope\nbark nope.x(1)\nwow\n");
        let source = std::fs::read_to_string(&entry).unwrap();

        let err = match load_program(&entry, &source) {
            Err(e) => e,
            Ok(_) => panic!("missing module should fail"),
        };
        assert_eq!(err.headline, "very import. much unknown.");
        assert_eq!(err.message, "doge has no module named nope");
    }

    #[test]
    fn single_file_program_rejects_an_unknown_stdlib_module() {
        let script = crate::parser::parse("t.doge", "so bogus\nwow\n").unwrap();
        let err = match single_file_program("t.doge", "so bogus\nwow\n", script) {
            Err(e) => e,
            Ok(_) => panic!("unknown module should fail"),
        };
        assert_eq!(err.headline, "very import. much unknown.");
        assert!(err.hint.as_deref().unwrap_or_default().contains("nerd"));
    }
}
