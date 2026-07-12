//! A tree-walking interpreter for Doge — the second execution engine beside the
//! Rust-transpiling compiler. It evaluates a checked AST directly against
//! `doge-runtime`, so `doge repl` (and the interpreter path in general) skips the
//! rustc build entirely. Every value operation calls the same `doge-runtime`
//! function the generated Rust would, so an interpreted program behaves
//! identically to a compiled one — a guarantee the examples parity suite enforces.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use doge_compiler as dc;
use doge_runtime::{DogeError, DogeResult, ErrorKind, Value};

mod analyze;
mod call;
mod exec;
mod expr;
mod natives;
#[cfg(test)]
mod tests;

pub use dc::{ClassInfo, ReplParse, SessionScope};

/// A shared, mutable binding cell — the interpreter's variables and the compiled
/// runtime's are the same `Rc<RefCell<Value>>`, so closures capture by sharing.
type Cell = doge_runtime::Cell;

/// A lexical scope: names bound in it to their shared cells. A function call gets
/// a fresh one seeded with its captures and parameters; a file's top level shares
/// one persistent scope (the REPL session's globals live here).
type Scope = Rc<RefCell<HashMap<String, Cell>>>;

fn scope() -> Scope {
    Rc::new(RefCell::new(HashMap::new()))
}

fn cell(value: Value) -> Cell {
    Rc::new(RefCell::new(value))
}

/// A resolved `so` import: a stdlib module, or a user module by file id.
#[derive(Clone, Copy)]
enum ModuleRef {
    Stdlib(&'static dc::Module),
    User(u32),
}

/// One source file's top-level scope: its persistent globals and its resolved
/// imports (local name → module).
struct FileScope {
    globals: Scope,
    imports: HashMap<String, ModuleRef>,
}

/// A user-defined function, method, or closure, keyed by a program-wide `fn_id`.
/// The `capture_names` name each captured cell a closure value carries, in the
/// same order, so a call can rebuild the closure's captured scope.
struct Template {
    name: String,
    file_id: u32,
    params: dc::Params,
    body: Rc<[dc::Stmt]>,
    capture_names: Vec<String>,
    /// The class a method belongs to (for `super`); `None` for a plain function.
    method_class: Option<u32>,
}

/// How a builtin/stdlib native is invoked: the exact runtime function it wires to
/// plus how many arguments it takes.
struct Native {
    name: String,
    runtime_fn: &'static str,
    arity: Arity,
}

/// A native's accepted argument count: a fixed number, `range`'s one-or-two, or
/// `gib`'s zero-or-one.
#[derive(Clone, Copy)]
enum Arity {
    Exact(usize),
    OneOrTwo,
    ZeroOrOne,
}

/// Anything callable through a `fn_id`: a user definition, a runtime native, or a
/// class constructor (a class name used as a value calls this to build an
/// instance, carrying the class's `class_id`).
enum Callable {
    User(Template),
    Native(Native),
    Ctor(u32),
}

/// One class, keyed by a program-wide `class_id`: its name, defining file, parent
/// (for inheritance and `super`), own methods (method name → `fn_id`), and the
/// `fn_id` of its constructor callable (for materializing the class as a value).
struct ClassData {
    name: String,
    file_id: u32,
    parent: Option<u32>,
    methods: HashMap<String, usize>,
    ctor_fn_id: usize,
}

/// Non-local control flow bubbling out of statement execution.
enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

/// An interpreter session. Holds the program-wide callable and class tables, one
/// scope per file, and the mutable run state (recursion depth, current location).
/// A `doge repl` session reuses one `Interp` across snippets, so bindings persist.
pub struct Interp {
    callables: Vec<Rc<Callable>>,
    classes: Vec<Rc<ClassData>>,
    file_scopes: Vec<FileScope>,
    /// Per file: top-level function name → `fn_id`, for hoisting function values.
    file_funcs: Vec<HashMap<String, usize>>,
    /// Per file: class name → `class_id`, for resolving constructors.
    file_class_ids: Vec<HashMap<String, u32>>,
    /// Builtin name → `fn_id`; stable for the session so function values keep working.
    builtin_ids: HashMap<String, usize>,
    /// (module, member) → `fn_id` for stdlib module functions.
    module_fn_ids: HashMap<(String, String), usize>,
    /// One path per file id, for rendering error locations.
    file_paths: Vec<Rc<str>>,
    depth: usize,
    cur_fid: u32,
    cur_line: u32,
    /// The class whose method body is currently running, for `super` resolution.
    current_method_class: Option<u32>,
    /// A module constant initializer that failed during integration; surfaced
    /// when the entry runs, matching the compiled program's initialization order.
    pending_module_error: Option<DogeError>,
    /// Names declared with `so … =` in the entry scope, so the REPL's checker keeps
    /// rejecting reassignment to them across snippets.
    session_consts: Vec<String>,
}

/// Run a whole loaded program to completion: initialize every module, then
/// execute the entry file's top-level statements. Used to run a `.doge` file
/// through the interpreter (the parity path beside `doge build`).
pub fn run_program(program: &dc::Program) -> DogeResult<()> {
    let mut interp = Interp::new();
    interp.run(program)
}

impl Default for Interp {
    fn default() -> Self {
        Interp::new()
    }
}

impl Interp {
    /// Integrate a loaded program and run its entry file to completion.
    pub fn run(&mut self, program: &dc::Program) -> DogeResult<()> {
        self.integrate_program(program);
        self.run_entry(program)
    }

    /// Integrate a loaded program *without* running its entry body — the setup the
    /// test runner needs before it drives individual `test`-prefixed functions with
    /// [`call_entry_function`]. A module constant initializer that failed during
    /// integration surfaces here, just as it would when the entry runs.
    pub fn prepare(&mut self, program: &dc::Program) -> DogeResult<()> {
        self.integrate_program(program);
        match self.pending_module_error.take() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// The file id and line the interpreter last executed — the site of an uncaught
    /// error, for the caller to render a doge-flavored location.
    pub fn error_site(&self) -> (usize, u32) {
        (self.cur_fid as usize, self.cur_line)
    }

    /// A fresh session with only the runtime natives (builtins + stdlib) registered.
    pub fn new() -> Interp {
        let mut interp = Interp {
            callables: Vec::new(),
            classes: Vec::new(),
            file_scopes: vec![FileScope {
                globals: scope(),
                imports: HashMap::new(),
            }],
            file_funcs: vec![HashMap::new()],
            file_class_ids: vec![HashMap::new()],
            builtin_ids: HashMap::new(),
            module_fn_ids: HashMap::new(),
            file_paths: vec![Rc::from("<repl>")],
            depth: 0,
            cur_fid: 0,
            cur_line: 0,
            current_method_class: None,
            pending_module_error: None,
            session_consts: Vec::new(),
        };
        interp.register_natives();
        interp
    }

    // ----- file-scope helpers -----

    fn globals(&self, fid: u32) -> Scope {
        self.file_scopes[fid as usize].globals.clone()
    }

    fn import_ref(&self, fid: u32, name: &str) -> Option<ModuleRef> {
        self.file_scopes[fid as usize].imports.get(name).copied()
    }

    /// Look up a name in a call frame, then the file's globals.
    fn lookup(&self, frame: &Scope, fid: u32, name: &str) -> Option<Cell> {
        if let Some(c) = frame.borrow().get(name) {
            return Some(c.clone());
        }
        self.file_scopes[fid as usize]
            .globals
            .borrow()
            .get(name)
            .cloned()
    }

    /// The class defined as `name` in file `fid`, if any.
    fn class_id_in(&self, fid: u32, name: &str) -> Option<u32> {
        self.file_class_ids[fid as usize].get(name).copied()
    }

    /// Track the current source location so an uncaught or caught error reports
    /// the file and line it was raised at, exactly as the compiled program's
    /// `cur_file`/`cur_line` do.
    fn mark(&mut self, fid: u32, span: dc::Span) {
        self.cur_fid = fid;
        self.cur_line = span.line;
    }

    /// The path of the file whose code is currently executing, for error values.
    fn cur_path(&self) -> Rc<str> {
        self.file_paths
            .get(self.cur_fid as usize)
            .cloned()
            .unwrap_or_else(|| Rc::from("<repl>"))
    }

    // ----- program integration -----

    /// Fold a loaded program's files into the session: create each file's scope
    /// and imports, analyze every definition into the callable/class tables, hoist
    /// top-level functions and classes, and initialize module constants.
    fn integrate_program(&mut self, program: &dc::Program) {
        // File 0 already has a scope (the session's entry/globals); add the rest.
        for file in &program.files {
            let fid = file.file_id as usize;
            while self.file_scopes.len() <= fid {
                self.file_scopes.push(FileScope {
                    globals: scope(),
                    imports: HashMap::new(),
                });
                self.file_funcs.push(HashMap::new());
                self.file_class_ids.push(HashMap::new());
                self.file_paths.push(Rc::from("<repl>"));
            }
            self.file_paths[fid] = Rc::from(file.path.as_str());
            self.resolve_imports(file);
        }
        for file in &program.files {
            self.analyze_file(&file.script.stmts, file.file_id);
            self.hoist_file(&file.script.stmts, file.file_id);
        }
        // Module constants first, in dependency order, so a module that reads
        // another's constant finds it ready — then the entry runs inline later.
        for &fid in &program.init_order {
            let stmts = program.files[fid as usize].script.stmts.clone();
            if let Err(err) = self.init_module(&stmts, fid) {
                // A module's constant initializer failing is a program error; it
                // surfaces when the entry runs, matching the compiled init order.
                self.pending_module_error = Some(err);
                break;
            }
        }
    }

    /// Resolve one file's `so` imports into its scope's import map.
    fn resolve_imports(&mut self, file: &dc::ProgramFile) {
        let fid = file.file_id as usize;
        for (name, module) in &file.stdlib_imports {
            self.file_scopes[fid]
                .imports
                .insert(name.clone(), ModuleRef::Stdlib(module));
        }
        for (name, target) in &file.user_imports {
            self.file_scopes[fid]
                .imports
                .insert(name.clone(), ModuleRef::User(*target));
        }
    }

    /// Bind a file's top-level functions to function values and pre-bind its
    /// hoisted variable names to `none`, mirroring the compiler's `Env` fields.
    fn hoist_file(&mut self, stmts: &[dc::Stmt], fid: u32) {
        let globals = self.globals(fid);
        for name in dc::hoisted_names(stmts) {
            globals
                .borrow_mut()
                .entry(name)
                .or_insert_with(|| cell(Value::None));
        }
        self.hoist_functions(stmts, &globals, fid);
    }

    /// Run one module's constant initializers in its own scope.
    fn init_module(&mut self, stmts: &[dc::Stmt], fid: u32) -> DogeResult<()> {
        let globals = self.globals(fid);
        for stmt in stmts {
            if matches!(stmt, dc::Stmt::ConstDecl { .. }) {
                self.exec_stmt(stmt, &globals, fid)?;
            }
        }
        Ok(())
    }

    /// Execute the entry file's top-level statements (skipping definitions and
    /// imports, which are already integrated), returning any uncaught error.
    fn run_entry(&mut self, program: &dc::Program) -> DogeResult<()> {
        if let Some(err) = self.pending_module_error.take() {
            return Err(err);
        }
        let entry = &program.files[0];
        let globals = self.globals(0);
        for stmt in &entry.script.stmts {
            if matches!(
                stmt,
                dc::Stmt::FuncDef { .. } | dc::Stmt::ObjDef { .. } | dc::Stmt::Import { .. }
            ) {
                continue;
            }
            match self.exec_stmt(stmt, &globals, 0)? {
                Flow::Normal => {}
                // `return`/`bork`/`continue` at the top level are rejected by the
                // checker, so reaching here would be a checked-away impossibility.
                _ => break,
            }
        }
        Ok(())
    }

    // ----- REPL session -----

    /// The session's accumulated top-level scope, for seeding the checker of the
    /// next snippet. Built from live interpreter state so the checker and runtime
    /// never disagree about what is in scope.
    pub fn session_scope(&self) -> SessionScope {
        let mut globals: Vec<String> = self.file_scopes[0]
            .globals
            .borrow()
            .keys()
            .cloned()
            .collect();
        globals.extend(self.file_scopes[0].imports.keys().cloned());
        globals.extend(self.file_class_ids[0].keys().cloned());
        let classes = self.file_class_ids[0]
            .iter()
            .map(|(name, id)| {
                let data = &self.classes[*id as usize];
                ClassInfo {
                    name: name.clone(),
                    parent: data.parent.map(|p| self.classes[p as usize].name.clone()),
                    methods: data.methods.keys().cloned().collect(),
                }
            })
            .collect();
        SessionScope {
            globals,
            consts: self.session_consts.clone(),
            classes,
        }
    }

    /// Evaluate one checked REPL snippet in the session: integrate its definitions,
    /// run its statements, and return the value of a trailing bare expression for
    /// the prompt to echo (`None` when the last statement is not a bare expression).
    pub fn eval_snippet(&mut self, path: &str, script: &dc::Script) -> DogeResult<Option<Value>> {
        self.file_paths[0] = Rc::from(path);
        for stmt in &script.stmts {
            if let dc::Stmt::Import { module, .. } = stmt {
                self.register_repl_import(module)?;
            }
        }
        self.analyze_file(&script.stmts, 0);
        self.hoist_file(&script.stmts, 0);

        let globals = self.globals(0);
        let count = script.stmts.len();
        for (i, stmt) in script.stmts.iter().enumerate() {
            match stmt {
                dc::Stmt::FuncDef { .. } | dc::Stmt::ObjDef { .. } | dc::Stmt::Import { .. } => {}
                dc::Stmt::ExprStmt { expr } if i + 1 == count => {
                    self.mark(0, expr.span());
                    return Ok(Some(self.eval(expr, &globals, 0)?));
                }
                dc::Stmt::ConstDecl { name, .. } => {
                    self.exec_stmt(stmt, &globals, 0)?;
                    if !self.session_consts.iter().any(|c| c == name) {
                        self.session_consts.push(name.clone());
                    }
                }
                _ => {
                    self.exec_stmt(stmt, &globals, 0)?;
                }
            }
        }
        Ok(None)
    }

    /// Register a `so` import encountered in a REPL snippet. Stdlib modules bind
    /// into the entry scope; user modules are only available when running a file.
    fn register_repl_import(&mut self, module: &str) -> DogeResult<()> {
        if self.file_scopes[0].imports.contains_key(module) {
            return Ok(());
        }
        match dc::stdlib_module(module) {
            Some(m) => {
                self.file_scopes[0]
                    .imports
                    .insert(module.to_string(), ModuleRef::Stdlib(m));
                Ok(())
            }
            None => Err(DogeError::new(
                ErrorKind::ValueError,
                format!(
                    "user modules aren't available in the repl yet — run the file instead (doge bark <script>.doge) to use {module}"
                ),
            )),
        }
    }
}
