use super::*;
use crate::modules::ProgramFile;

impl Codegen {
    /// The driver that assembles the whole Rust source: gather per-file analysis,
    /// then emit each section (run body, functions, objects, closures, source
    /// tables, `Env`/`main`, dispatchers) and concatenate them in a fixed order.
    pub(super) fn program(&self, program: &Program) -> Result<String, Diagnostic> {
        let tables: Vec<FileTable> = program.files.iter().map(file_table).collect();
        let analysis = analyze_program(program);
        let classes = self.collect_classes(&program.files[0]);
        let env_fields = self.env_fields(program, &tables);

        let mut emit = Emit {
            tables: &tables,
            file_id: 0,
            classes: &classes,
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

    /// Objects are entry-only (a module with an object is a check error), so the
    /// class list comes from the entry alone; each object's source-order index is
    /// its class id.
    fn collect_classes(&self, entry: &ProgramFile) -> Vec<Class> {
        let mut classes: Vec<Class> = Vec::new();
        for stmt in &entry.script.stmts {
            if let Stmt::ObjDef { name, methods, .. } = stmt {
                let methods = methods
                    .iter()
                    .filter_map(|m| match m {
                        Stmt::FuncDef { name, params, .. } => Some((name.clone(), params.clone())),
                        _ => None,
                    })
                    .collect();
                classes.push(Class {
                    name: name.clone(),
                    id: classes.len() as u32,
                    methods,
                });
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

    /// Each entry object contributes a constructor plus a wrapper/body pair per
    /// method; the source is looked up from the `ObjDef`, keyed by class id.
    fn emit_objects(
        &self,
        program: &Program,
        classes: &[Class],
        emit: &mut Emit,
        out: &mut String,
    ) -> Result<(), Diagnostic> {
        let entry = &program.files[0];
        self.enter_file(emit, 0);
        for (class, stmt) in classes.iter().zip(
            entry
                .script
                .stmts
                .iter()
                .filter(|s| matches!(s, Stmt::ObjDef { .. })),
        ) {
            let Stmt::ObjDef { methods, .. } = stmt else {
                unreachable!("compiler bug: class list and ObjDef filter disagree")
            };
            self.constructor(class, out);
            for method in methods {
                if let Stmt::FuncDef { span, .. } = method {
                    self.method(class, *span, emit, out)?;
                }
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
