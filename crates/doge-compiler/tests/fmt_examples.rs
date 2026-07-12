use std::path::PathBuf;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

/// Every checked-in example is already in canonical form, so formatting it must
/// be a no-op. This pins the formatter's output to the style the examples define.
#[test]
fn examples_are_already_formatted() {
    let mut checked = 0;
    for entry in std::fs::read_dir(examples_dir()).expect("examples dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("doge") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("readable example");
        let formatted = doge_compiler::format(path.to_str().unwrap(), &source)
            .unwrap_or_else(|d| panic!("{} should format:\n{}", path.display(), d.render()));
        assert_eq!(
            formatted,
            source,
            "{} is not in canonical form",
            path.display()
        );
        checked += 1;
    }
    assert!(checked > 0, "no examples were checked");
}
