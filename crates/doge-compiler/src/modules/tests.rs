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
