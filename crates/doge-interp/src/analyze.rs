//! The static pass that turns a file's statements into the interpreter's
//! callable and class tables. It mirrors the compiler's whole-program analysis:
//! every function definition gets a program-wide `fn_id`, every closure records
//! the enclosing names it captures, and every object becomes a `ClassData` with a
//! program-wide `class_id` and its parent resolved within its file. Runs
//! incrementally — a REPL snippet appends new ids without renumbering old ones, so
//! function values created by earlier snippets keep working.

use std::collections::HashSet;
use std::rc::Rc;

use doge_compiler as dc;
use doge_runtime::Value;

use crate::{cell, Callable, ClassData, Interp, Scope, Template};

impl Interp {
    /// Analyze every top-level definition in `stmts` (belonging to file `fid`):
    /// register top-level functions, object classes (with methods), and closures.
    pub(crate) fn analyze_file(&mut self, stmts: &[dc::Stmt], fid: u32) {
        let empty = HashSet::new();

        for stmt in stmts {
            if let dc::Stmt::FuncDef {
                name, params, body, ..
            } = stmt
            {
                let id = self.analyze_fn(name, params, body, fid, None, &empty);
                self.file_funcs[fid as usize].insert(name.clone(), id);
            }
        }

        // Objects: assign class ids first (a parent may be declared later in the
        // file), then resolve parents, then register each method as a template.
        let mut new_classes: Vec<(u32, Option<String>, &[dc::Stmt])> = Vec::new();
        for stmt in stmts {
            if let dc::Stmt::ObjDef {
                name,
                parent,
                methods,
                ..
            } = stmt
            {
                let class_id = self.classes.len() as u32;
                self.classes.push(Rc::new(ClassData {
                    name: name.clone(),
                    file_id: fid,
                    parent: None,
                    methods: std::collections::HashMap::new(),
                    ctor_fn_id: 0,
                }));
                self.file_class_ids[fid as usize].insert(name.clone(), class_id);
                new_classes.push((class_id, parent.clone(), methods));
            }
        }
        for (class_id, parent, methods) in new_classes {
            let parent_id = parent.and_then(|p| self.class_id_in(fid, &p));
            let mut method_ids = std::collections::HashMap::new();
            for method in methods {
                if let dc::Stmt::FuncDef {
                    name, params, body, ..
                } = method
                {
                    let id = self.analyze_fn(name, params, body, fid, Some(class_id), &empty);
                    method_ids.insert(name.clone(), id);
                }
            }
            // Register the constructor as a callable so a class name used as a
            // value dispatches here to build an instance.
            let ctor_fn_id = self.callables.len();
            self.callables.push(Rc::new(Callable::Ctor(class_id)));
            self.classes[class_id as usize] = Rc::new(ClassData {
                name: self.classes[class_id as usize].name.clone(),
                file_id: fid,
                parent: parent_id,
                methods: method_ids,
                ctor_fn_id,
            });
        }

        // Closures nested inside top-level blocks capture nothing (their enclosing
        // scope is the file's run body, which holds no cells).
        for stmt in stmts {
            if matches!(
                stmt,
                dc::Stmt::If { .. }
                    | dc::Stmt::For { .. }
                    | dc::Stmt::While { .. }
                    | dc::Stmt::Try { .. }
            ) {
                for (name, params, body, _) in dc::child_funcdefs(std::slice::from_ref(stmt)) {
                    self.analyze_fn(name, params, body, fid, None, &empty);
                }
            }
        }
    }

    /// Analyze one function/method/closure: record its capture names (the free
    /// names that are cells in the enclosing frame), push its template, and recurse
    /// into its own nested functions. Returns the assigned `fn_id`.
    fn analyze_fn(
        &mut self,
        name: &str,
        params: &dc::Params,
        body: &[dc::Stmt],
        fid: u32,
        method_class: Option<u32>,
        enclosing_cells: &HashSet<String>,
    ) -> usize {
        let mut binding = Vec::new();
        if method_class.is_some() {
            binding.push("self".to_string());
        }
        binding.extend(params.binding_names());

        let mut capture_names: Vec<String> = dc::free_names(&binding, body)
            .into_iter()
            .filter(|n| enclosing_cells.contains(n))
            .collect();
        capture_names.sort();

        let mut cell_names = dc::celled_locals(&binding, body);
        cell_names.extend(capture_names.iter().cloned());

        let id = self.callables.len();
        self.callables.push(Rc::new(Callable::User(Template {
            name: name.to_string(),
            file_id: fid,
            params: params.clone(),
            body: Rc::from(body.to_vec().into_boxed_slice()),
            capture_names,
            method_class,
        })));

        for (child_name, child_params, child_body, _) in dc::child_funcdefs(body) {
            self.analyze_fn(child_name, child_params, child_body, fid, None, &cell_names);
        }
        id
    }

    /// Bind every function directly defined in `stmts` (a top-level file scope, or
    /// a function body being entered) to a function value in `frame`, capturing the
    /// cells named by its template. Nested-in-block functions are bound when their
    /// definition statement runs, not here.
    pub(crate) fn hoist_functions(&mut self, stmts: &[dc::Stmt], frame: &Scope, fid: u32) {
        for stmt in stmts {
            if let dc::Stmt::FuncDef { name, span, .. } = stmt {
                let value = self.make_function(*span, name, frame, fid);
                frame.borrow_mut().insert(name.clone(), cell(value));
            }
        }
    }

    /// Build the function value for the definition of `name` at `span`: look up its
    /// analyzed `fn_id` and capture the cells its template names from `frame`.
    pub(crate) fn make_function(
        &self,
        _span: dc::Span,
        name: &str,
        frame: &Scope,
        fid: u32,
    ) -> Value {
        let id = self.fn_id_of(name, fid);
        let captures = self.capture_cells(id, frame, fid);
        Value::function(id as u32, name, captures)
    }

    /// The `fn_id` of the function named `name` visible in file `fid`. A top-level
    /// function is in the file table; a nested one shares its name only within its
    /// own scope, so the most recently analyzed matching template is used.
    fn fn_id_of(&self, name: &str, fid: u32) -> usize {
        if let Some(id) = self.file_funcs[fid as usize].get(name) {
            return *id;
        }
        // A nested function: find the last-analyzed user template with this name in
        // this file. Analysis appends in definition order, so the last match is the
        // one being defined here.
        self.callables
            .iter()
            .enumerate()
            .rev()
            .find_map(|(id, c)| match c.as_ref() {
                Callable::User(t) if t.name == name && t.file_id == fid => Some(id),
                _ => None,
            })
            .expect("interp bug: function definition was not analyzed")
    }

    /// The captured cells a closure value carries: one per capture name, read from
    /// the defining `frame` (falling back to file globals for a hoisted name).
    fn capture_cells(&self, id: usize, frame: &Scope, fid: u32) -> Vec<crate::Cell> {
        let Callable::User(template) = self.callables[id].as_ref() else {
            return Vec::new();
        };
        template
            .capture_names
            .iter()
            .map(|name| {
                self.lookup(frame, fid, name)
                    .unwrap_or_else(|| cell(Value::None))
            })
            .collect()
    }
}
