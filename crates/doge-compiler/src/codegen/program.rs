use super::*;

impl Codegen {
    /// The driver that assembles the whole Rust source: gather per-file analysis,
    /// then emit each section (run body, functions, objects, closures, source
    /// tables, `Env`/`main`, dispatchers) and concatenate them in a fixed order.
    pub(super) fn program(&self, program: &Program) -> Result<String, Diagnostic> {
        let tables: Vec<FileTable> = program.files.iter().map(file_table).collect();
        let analysis = analyze_program(program);
        let classes = self.collect_classes(program);
        let env_fields = self.env_fields(program, &tables);

        let mut emit = Emit {
            tables: &tables,
            file_id: 0,
            classes: &classes,
            current_class: None,
            analysis: &analysis,
            uses_method_call: Cell::new(false),
            uses_call_function: Cell::new(false),
            materialized: RefCell::new(HashSet::new()),
            locals: HashMap::new(),
            local_funcs: HashMap::new(),
            counter: 0,
            try_stack: Vec::new(),
            loop_stack: Vec::new(),
        };

        let run_body = self.emit_run_body(program, &mut emit)?;

        let mut funcs_src = String::new();
        self.emit_toplevel_funcs(program, &mut emit, &mut funcs_src)?;
        self.emit_objects(program, &classes, &mut emit, &mut funcs_src)?;
        self.emit_closures(&analysis, &mut emit, &mut funcs_src)?;

        // Dispatcher arms emit parameter defaults, which are self-contained
        // literals: clear any leftover try/loop context so a fallible default
        // (a dict literal) propagates with `?` rather than a stray break label.
        self.enter_file(&mut emit, 0);
        emit.try_stack.clear();
        emit.loop_stack.clear();
        let dispatcher = self.dispatcher(&classes, emit.uses_method_call.get(), &emit)?;
        let fn_dispatcher = self.function_dispatcher(&emit)?;

        let mut out = String::new();
        out.push_str("#![allow(warnings)]\n");
        out.push_str("use doge_runtime::*;\n\n");
        self.emit_source_tables(&mut out);
        self.emit_env_and_main(&env_fields, &mut out);

        out.push_str("fn run(env: &mut Env) -> DogeResult<()> {\n");
        out.push_str(&run_body);
        out.push_str("    Ok(())\n");
        out.push_str("}\n");

        out.push_str(&funcs_src);
        out.push_str(&dispatcher);
        out.push_str(&fn_dispatcher);
        Ok(out)
    }

    /// Every object in the program, across all files, each with a program-wide
    /// class id (its position in this list). The entry (file 0) is walked first, so
    /// its ids are unchanged from the single-file case; a module's objects follow.
    fn collect_classes(&self, program: &Program) -> Vec<Class> {
        let mut classes: Vec<Class> = Vec::new();
        // Parent names, parallel to `classes` by class id, resolved once every
        // class has an id (a parent may be declared after the child in the file).
        let mut parent_names: Vec<Option<String>> = Vec::new();
        for file in &program.files {
            for stmt in &file.script.stmts {
                if let Stmt::ObjDef {
                    name,
                    parent,
                    methods,
                    ..
                } = stmt
                {
                    let methods = methods
                        .iter()
                        .filter_map(|m| match m {
                            Stmt::FuncDef { name, params, .. } => {
                                Some((name.clone(), params.clone()))
                            }
                            _ => None,
                        })
                        .collect();
                    classes.push(Class {
                        file_id: file.file_id,
                        name: name.clone(),
                        id: classes.len() as u32,
                        parent: None,
                        methods,
                    });
                    parent_names.push(parent.clone());
                }
            }
        }
        // A parent is a class of the same file — the checker guarantees it exists
        // and the chain is acyclic, so an unresolved name here is a checked program
        // and simply leaves `parent` as `None`.
        for id in 0..classes.len() {
            if let Some(parent) = &parent_names[id] {
                let file_id = classes[id].file_id;
                classes[id].parent = classes
                    .iter()
                    .find(|c| c.file_id == file_id && &c.name == parent)
                    .map(|c| c.id);
            }
        }
        classes
    }

    /// The `Env` fields: the line tracker and recursion depth live in `main`, and
    /// this adds every file's top-level bound names — the entry's `v_` fields,
    /// then each module's `g_` constant fields. A direct top-level function/object
    /// is a static definition, not a field.
    fn env_fields(&self, program: &Program, tables: &[FileTable]) -> Vec<String> {
        let entry = &program.files[0];
        let mut env_fields: Vec<String> = toplevel_hoisted(&entry.script.stmts)
            .iter()
            .map(|name| field_name(0, name))
            .collect();
        for file in &program.files[1..] {
            for name in &tables[file.file_id as usize].members {
                if tables[file.file_id as usize].consts.contains(name) {
                    env_fields.push(field_name(file.file_id, name));
                }
            }
        }
        env_fields
    }

    /// The body of `run`: first every module's constants in dependency order (so a
    /// module referencing another's constant sees it ready), then the entry's own
    /// top-level statements (skipping definitions and imports).
    fn emit_run_body(&self, program: &Program, emit: &mut Emit) -> Result<String, Diagnostic> {
        let mut run_body = String::new();
        for &fid in &program.init_order {
            self.enter_file(emit, fid);
            for stmt in &program.files[fid as usize].script.stmts {
                if matches!(stmt, Stmt::ConstDecl { .. }) {
                    self.stmt(stmt, 1, emit, &mut run_body)?;
                }
            }
        }
        self.enter_file(emit, 0);
        for stmt in &program.files[0].script.stmts {
            if matches!(
                stmt,
                Stmt::FuncDef { .. } | Stmt::ObjDef { .. } | Stmt::Import { .. }
            ) {
                continue;
            }
            self.stmt(stmt, 1, emit, &mut run_body)?;
        }
        Ok(run_body)
    }

    /// Every file's top-level functions, mangled by file id.
    fn emit_toplevel_funcs(
        &self,
        program: &Program,
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        for file in &program.files {
            self.enter_file(emit, file.file_id);
            for stmt in &file.script.stmts {
                if let Stmt::FuncDef { span, .. } = stmt {
                    self.function(*span, emit, out)?;
                }
            }
        }
        Ok(())
    }

    /// Each object contributes a constructor plus a wrapper/body pair per method,
    /// mangled by its program-wide class id. Objects from every file are emitted;
    /// `classes` is in file-then-source order, matching each file's `ObjDef` order.
    fn emit_objects(
        &self,
        program: &Program,
        classes: &[Class],
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        for file in &program.files {
            self.enter_file(emit, file.file_id);
            let file_classes = classes.iter().filter(|c| c.file_id == file.file_id);
            let objdefs = file
                .script
                .stmts
                .iter()
                .filter(|s| matches!(s, Stmt::ObjDef { .. }));
            for (class, stmt) in file_classes.zip(objdefs) {
                let Stmt::ObjDef { methods, .. } = stmt else {
                    unreachable!("compiler bug: class list and ObjDef filter disagree")
                };
                self.constructor(classes, class, out);
                emit.current_class = Some(class.id);
                for method in methods {
                    if let Stmt::FuncDef { span, .. } = method {
                        self.method(class, *span, emit, out)?;
                    }
                }
                emit.current_class = None;
            }
        }
        Ok(())
    }

    /// Every closure — nested functions, at any depth, in any file — emitted as a
    /// `c_`/`cb_` pair, ordered by id for stable output.
    fn emit_closures(
        &self,
        analysis: &Analysis,
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let mut closures: Vec<&FnInfo> = analysis
            .fn_info
            .values()
            .filter(|info| info.kind == FnKind::Closure)
            .collect();
        closures.sort_by_key(|info| info.fn_id);
        for info in closures {
            self.enter_file(emit, info.file_id);
            self.closure(info, emit, out)?;
        }
        Ok(())
    }
}
