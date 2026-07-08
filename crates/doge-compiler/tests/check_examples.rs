use std::path::PathBuf;

/// Absolute path to the repo-root `examples/` directory.
fn examples_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/doge-compiler; examples/ is two levels up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[test]
fn every_example_parses_and_checks() {
    let dir = examples_dir();
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("examples directory should exist") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("doge") {
            continue;
        }
        seen += 1;
        let name = path.display().to_string();
        let source = std::fs::read_to_string(&path).expect("readable example");

        let script = match doge_compiler::parse(&name, &source) {
            Ok(script) => script,
            Err(diag) => panic!("{name} should parse, but:\n{}", diag.render()),
        };
        if let Err(diag) = doge_compiler::check(&name, &source, &script) {
            panic!("{name} should check clean, but:\n{}", diag.render());
        }
    }
    // Guard against a silently wrong path finding zero files.
    assert!(seen > 0, "no .doge examples found in {}", dir.display());
}

#[test]
fn every_fixture_fails_with_the_expected_diagnostic() {
    // (fixture file, a substring the rendered diagnostic must contain)
    let cases = [
        ("missing_wow.doge", "missing wow"),
        ("tab_indent.doge", "very tab. much confuse."),
        ("const_reassign.doge", "very const. much fixed."),
        ("undeclared_name.doge", "very unknown. much name."),
        ("chained_comparison.doge", "chain comparisons"),
        ("bork_outside_loop.doge", "very bork. much nowhere."),
        ("python_def.doge", "very python. much habit."),
    ];

    let dir = fixtures_dir();
    for (file, expected) in cases {
        let path = dir.join(file);
        let source =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing fixture {file}"));

        // A fixture fails at either the parse or the check stage; run both.
        let rendered = match doge_compiler::parse(file, &source) {
            Err(diag) => diag.render(),
            Ok(script) => match doge_compiler::check(file, &source, &script) {
                Err(diag) => diag.render(),
                Ok(()) => panic!("{file} should have failed, but parsed and checked clean"),
            },
        };

        assert!(
            rendered.contains(expected),
            "{file}: expected diagnostic to contain {expected:?}, got:\n{rendered}"
        );
    }
}
