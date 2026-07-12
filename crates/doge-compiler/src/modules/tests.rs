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
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture dir");
        }
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
fn a_path_import_loads_a_module_from_a_subdirectory() {
    let dir = TempDir::new("path-subdir");
    dir.write(
        "lib/shibe_math.doge",
        "such square much n:\n    return n * n\nwow\nwow\n",
    );
    let entry = dir.write(
        "app.doge",
        "so \"lib/shibe_math.doge\"\nbark shibe_math.square(4)\nwow\n",
    );
    let source = std::fs::read_to_string(&entry).unwrap();

    let program = match load_program(&entry, &source) {
        Ok(program) => program,
        Err(e) => panic!("should load: {}", e.render()),
    };
    assert_eq!(program.files.len(), 2, "entry + one subdir module");
    assert_eq!(program.files[1].name, "shibe_math");
}

#[test]
fn a_path_import_can_climb_with_dotdot() {
    let dir = TempDir::new("path-dotdot");
    dir.write("shared.doge", "such id much n:\n    return n\nwow\nwow\n");
    dir.write(
        "app/main.doge",
        "so \"../shared.doge\"\nbark shared.id(7)\nwow\n",
    );
    let entry = dir.0.join("app/main.doge").to_string_lossy().into_owned();
    let source = std::fs::read_to_string(&entry).unwrap();

    let program = match load_program(&entry, &source) {
        Ok(program) => program,
        Err(e) => panic!("should load: {}", e.render()),
    };
    assert_eq!(program.files.len(), 2);
    assert_eq!(program.files[1].name, "shared");
}

#[test]
fn the_same_file_via_two_routes_loads_once() {
    let dir = TempDir::new("path-dedup");
    dir.write("lib/util.doge", "so K = 1\nwow\n");
    dir.write(
        "a.doge",
        "so \"lib/util.doge\"\nsuch fa:\n    return util.K\nwow\nwow\n",
    );
    dir.write(
        "b.doge",
        "so \"./lib/util.doge\"\nsuch fb:\n    return util.K\nwow\nwow\n",
    );
    let entry = dir.write("app.doge", "so a\nso b\nbark a.fa()\nwow\n");
    let source = std::fs::read_to_string(&entry).unwrap();

    let program = match load_program(&entry, &source) {
        Ok(program) => program,
        Err(e) => panic!("should load: {}", e.render()),
    };
    // entry + a + b + util, with util (reached as "lib/util.doge" and
    // "./lib/util.doge") loaded a single time.
    assert_eq!(program.files.len(), 4);
}

#[test]
fn two_different_files_may_share_a_stem() {
    let dir = TempDir::new("path-samestem");
    dir.write("one/util.doge", "so K = 1\nwow\n");
    dir.write("two/util.doge", "so K = 2\nwow\n");
    let entry = dir.write(
        "app.doge",
        "so \"one/util.doge\"\nso \"two/util.doge\"\nbark 1\nwow\n",
    );
    let source = std::fs::read_to_string(&entry).unwrap();

    // The two modules share the stem `util`; they are distinct files, so both
    // load. (A duplicate binding name in one file is a separate check.)
    let program = match load_program(&entry, &source) {
        Ok(program) => program,
        Err(e) => panic!("should load: {}", e.render()),
    };
    assert_eq!(program.files.len(), 3, "entry + two distinct util modules");
}

#[test]
fn a_cycle_through_a_path_import_is_reported() {
    let dir = TempDir::new("path-cycle");
    dir.write(
        "lib/a.doge",
        "so \"b.doge\"\nsuch fa:\n    return 1\nwow\nwow\n",
    );
    dir.write(
        "lib/b.doge",
        "so \"a.doge\"\nsuch fb:\n    return 1\nwow\nwow\n",
    );
    let entry = dir.write("app.doge", "so \"lib/a.doge\"\nbark a.fa()\nwow\n");
    let source = std::fs::read_to_string(&entry).unwrap();

    let err = match load_program(&entry, &source) {
        Err(e) => e,
        Ok(_) => panic!("a cycle should fail"),
    };
    assert_eq!(err.headline, "very loop. much import.");
    assert!(err.message.contains("import cycle"));
}

#[test]
fn a_missing_path_module_names_the_path() {
    let dir = TempDir::new("path-missing");
    let entry = dir.write("app.doge", "so \"lib/nope.doge\"\nbark 1\nwow\n");
    let source = std::fs::read_to_string(&entry).unwrap();

    let err = match load_program(&entry, &source) {
        Err(e) => e,
        Ok(_) => panic!("missing path should fail"),
    };
    assert_eq!(err.headline, "very import. much unknown.");
    assert!(err.message.contains("lib/nope.doge"));
}

#[test]
fn importing_the_entry_script_is_rejected() {
    let dir = TempDir::new("path-self");
    let entry = dir.write("app.doge", "so \"app.doge\"\nbark 1\nwow\n");
    let source = std::fs::read_to_string(&entry).unwrap();

    let err = match load_program(&entry, &source) {
        Err(e) => e,
        Ok(_) => panic!("importing the entry should fail"),
    };
    assert_eq!(err.headline, "very import. much self.");
}

#[test]
fn a_path_import_stem_may_not_shadow_a_stdlib_module() {
    let dir = TempDir::new("path-shadow");
    dir.write("lib/nerd.doge", "so K = 1\nwow\n");
    let entry = dir.write("app.doge", "so \"lib/nerd.doge\"\nbark 1\nwow\n");
    let source = std::fs::read_to_string(&entry).unwrap();

    let err = match load_program(&entry, &source) {
        Err(e) => e,
        Ok(_) => panic!("shadowing a stdlib module should fail"),
    };
    assert_eq!(err.headline, "very shadow. much confuse.");
}

#[test]
fn a_dependency_import_resolves_to_its_entry() {
    let dir = TempDir::new("dep-basic");
    dir.write(
        "dep/doge.toml",
        "[package]\nname = \"greet\"\nentry = \"main.doge\"\n",
    );
    dir.write("dep/main.doge", "such hi much n:\n    return n\nwow\nwow\n");
    dir.write(
        "app/doge.toml",
        "[package]\nname = \"app\"\n\n[dependencies]\ngreet = { path = \"../dep\" }\n",
    );
    let entry = dir.write("app/main.doge", "so greet\nbark greet.hi(1)\nwow\n");
    let source = std::fs::read_to_string(&entry).unwrap();

    let app_root = std::fs::canonicalize(dir.0.join("app")).unwrap();
    let dep_root = std::fs::canonicalize(dir.0.join("dep")).unwrap();
    let dep_entry = std::fs::canonicalize(dir.0.join("dep/main.doge")).unwrap();
    let mut deps = DependencyMap::new();
    deps.insert(
        app_root,
        std::collections::HashMap::from([("greet".to_string(), dep_entry)]),
    );
    deps.insert(dep_root, std::collections::HashMap::new());

    let program = match load_program_with_deps(&entry, &source, deps) {
        Ok(program) => program,
        Err(e) => panic!("should load: {}", e.render()),
    };
    assert_eq!(program.files.len(), 2, "entry + the dependency's entry");
    assert_eq!(program.files[1].name, "greet");
}

#[test]
fn a_dependency_colliding_with_a_sibling_is_ambiguous() {
    let dir = TempDir::new("dep-conflict");
    dir.write("dep/doge.toml", "[package]\nname = \"greet\"\n");
    dir.write("dep/main.doge", "such hi:\n    return 1\nwow\nwow\n");
    // A sibling file of the same name as the dependency alias.
    dir.write("app/greet.doge", "such hi:\n    return 2\nwow\nwow\n");
    dir.write(
        "app/doge.toml",
        "[package]\nname = \"app\"\n\n[dependencies]\ngreet = { path = \"../dep\" }\n",
    );
    let entry = dir.write("app/main.doge", "so greet\nbark greet.hi()\nwow\n");
    let source = std::fs::read_to_string(&entry).unwrap();

    let app_root = std::fs::canonicalize(dir.0.join("app")).unwrap();
    let dep_entry = std::fs::canonicalize(dir.0.join("dep/main.doge")).unwrap();
    let mut deps = DependencyMap::new();
    deps.insert(
        app_root,
        std::collections::HashMap::from([("greet".to_string(), dep_entry)]),
    );

    let err = match load_program_with_deps(&entry, &source, deps) {
        Err(e) => e,
        Ok(_) => panic!("a dependency/sibling collision should fail"),
    };
    assert_eq!(err.headline, "very ambiguous. much confuse.");
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
