//! Parsing `doge.toml`, a project's manifest. A manifest names the package and
//! lists its dependencies (by local path or git). The format is a small, strict
//! subset of TOML — `[package]`/`[dependencies]` tables, `key = "string"` pairs,
//! and inline-table dependency values — so this parser stays hand-written and
//! doge-compiler keeps zero third-party dependencies. Every failure is a
//! doge-flavored [`Diagnostic`] anchored at the offending line.

use crate::diagnostics::{source_line, split_source_lines, Diagnostic};

/// The manifest file name discovered at a project root.
pub const MANIFEST_NAME: &str = "doge.toml";

/// The headline for every manifest problem.
const MANIFEST_HEADLINE: &str = "very manifest. much confuse.";

/// A parsed `doge.toml`.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    /// The package name (also the default binary name for `doge build`).
    pub name: String,
    /// The package version. Defaults to `0.0.0` when omitted.
    pub version: String,
    /// The entry script, relative to the project root. Defaults to `main.doge`.
    pub entry: String,
    /// Declared dependencies, in file order.
    pub dependencies: Vec<Dependency>,
}

/// One `[dependencies]` entry: a local alias bound by `so <alias>` and where its
/// package lives.
#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    /// The alias `so <alias>` imports; also the module binding name.
    pub alias: String,
    /// Where the dependency's package is fetched from.
    pub source: DependencySource,
    /// 1-based line in `doge.toml`, so resolution errors point at the right line.
    pub line: u32,
}

/// Where a dependency's package comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum DependencySource {
    /// A directory relative to the declaring package's root.
    Path(String),
    /// A git repository, pinned to a revision.
    Git { url: String, rev: GitRev },
}

/// Which git revision a git dependency resolves to.
#[derive(Debug, Clone, PartialEq)]
pub enum GitRev {
    /// No `rev`/`tag`/`branch`: the repository's default branch.
    Default,
    /// A commit sha (`rev = "…"`).
    Rev(String),
    /// A tag (`tag = "…"`).
    Tag(String),
    /// A branch (`branch = "…"`).
    Branch(String),
}

impl GitRev {
    /// The git ref to check out, or `None` for the default branch.
    pub fn as_ref_name(&self) -> Option<&str> {
        match self {
            GitRev::Default => None,
            GitRev::Rev(r) | GitRev::Tag(r) | GitRev::Branch(r) => Some(r),
        }
    }
}

/// The section a line belongs to while parsing.
enum Section {
    /// Before any `[table]` header.
    None,
    Package,
    Dependencies,
}

/// Parse `doge.toml` source (named `path` for diagnostics) into a [`Manifest`].
pub fn parse(path: &str, source: &str) -> Result<Manifest, Diagnostic> {
    let lines = split_source_lines(source);
    let err = |line: u32, message: &str, hint: &str| {
        Diagnostic::new(path, line, 1, source_line(&lines, line), message)
            .with_headline(MANIFEST_HEADLINE)
            .with_hint(hint)
    };

    let mut section = Section::None;
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut entry: Option<String> = None;
    let mut dependencies: Vec<Dependency> = Vec::new();

    for (index, raw) in lines.iter().enumerate() {
        let line_no = (index + 1) as u32;
        let text = strip_comment(raw).trim();
        if text.is_empty() {
            continue;
        }

        if let Some(header) = text.strip_prefix('[') {
            let header = header.strip_suffix(']').ok_or_else(|| {
                err(
                    line_no,
                    "a section header needs a closing ]",
                    "write [package] or [dependencies]",
                )
            })?;
            section = match header.trim() {
                "package" => Section::Package,
                "dependencies" => Section::Dependencies,
                other => {
                    return Err(err(
                        line_no,
                        &format!("doge does not know the section [{other}]"),
                        "manifests have [package] and [dependencies]",
                    ))
                }
            };
            continue;
        }

        let (key, value) = split_key_value(text).ok_or_else(|| {
            err(
                line_no,
                "expected a key = value line",
                "write name = \"my_app\" under [package]",
            )
        })?;

        match section {
            Section::None => {
                return Err(err(
                    line_no,
                    "this line is not under a [section]",
                    "start the file with [package]",
                ))
            }
            Section::Package => {
                let string = parse_string(value).ok_or_else(|| {
                    err(
                        line_no,
                        &format!("{key} must be a quoted string"),
                        "quote the value, e.g. name = \"my_app\"",
                    )
                })?;
                match key {
                    "name" => name = Some(string),
                    "version" => version = Some(string),
                    "entry" => entry = Some(string),
                    other => {
                        return Err(err(
                            line_no,
                            &format!("[package] has no key named {other}"),
                            "known keys: name, version, entry",
                        ))
                    }
                }
            }
            Section::Dependencies => {
                let source = parse_dependency(value, line_no, &err)?;
                dependencies.push(Dependency {
                    alias: key.to_string(),
                    source,
                    line: line_no,
                });
            }
        }
    }

    let name = name.ok_or_else(|| {
        Diagnostic::new(
            path,
            1,
            1,
            source_line(&lines, 1),
            "a manifest needs a package name",
        )
        .with_headline(MANIFEST_HEADLINE)
        .with_hint("add [package] with name = \"my_app\"")
    })?;

    Ok(Manifest {
        name,
        version: version.unwrap_or_else(|| "0.0.0".to_string()),
        entry: entry.unwrap_or_else(|| "main.doge".to_string()),
        dependencies,
    })
}

/// Drop a trailing `# comment` from a line, ignoring `#` inside a quoted string.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    for (i, ch) in line.char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Split `key = value` on the first top-level `=`, returning the trimmed halves.
fn split_key_value(text: &str) -> Option<(&str, &str)> {
    let eq = text.find('=')?;
    let key = text[..eq].trim();
    let value = text[eq + 1..].trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

/// Parse a double-quoted string. The subset has no escapes, so a value may not
/// contain a `"` (paths and URLs never do).
fn parse_string(raw: &str) -> Option<String> {
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    if inner.contains('"') {
        return None;
    }
    Some(inner.to_string())
}

/// Parse a dependency inline table `{ path = "…" }` or `{ git = "…", tag = "…" }`.
fn parse_dependency(
    value: &str,
    line: u32,
    err: &impl Fn(u32, &str, &str) -> Diagnostic,
) -> Result<DependencySource, Diagnostic> {
    let inner = value
        .strip_prefix('{')
        .and_then(|v| v.strip_suffix('}'))
        .ok_or_else(|| {
            err(
                line,
                "a dependency is an inline table",
                "write greet = { path = \"lib/greet\" }",
            )
        })?;

    let mut path = None;
    let mut git = None;
    let mut rev = None;
    let mut tag = None;
    let mut branch = None;

    for pair in inner.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (key, raw) = split_key_value(pair).ok_or_else(|| {
            err(
                line,
                "each dependency field is key = \"value\"",
                "write path = \"lib/greet\" or git = \"https://…\"",
            )
        })?;
        let string = parse_string(raw).ok_or_else(|| {
            err(
                line,
                &format!("{key} must be a quoted string"),
                "quote the value, e.g. path = \"lib/greet\"",
            )
        })?;
        let slot = match key {
            "path" => &mut path,
            "git" => &mut git,
            "rev" => &mut rev,
            "tag" => &mut tag,
            "branch" => &mut branch,
            other => {
                return Err(err(
                    line,
                    &format!("a dependency has no field named {other}"),
                    "fields: path, git, rev, tag, branch",
                ))
            }
        };
        *slot = Some(string);
    }

    match (path, git) {
        (Some(_), Some(_)) => Err(err(
            line,
            "a dependency is either a path or a git source, not both",
            "keep one of path or git",
        )),
        (Some(path), None) => {
            if rev.is_some() || tag.is_some() || branch.is_some() {
                return Err(err(
                    line,
                    "rev/tag/branch only apply to a git dependency",
                    "drop them, or switch path to git",
                ));
            }
            Ok(DependencySource::Path(path))
        }
        (None, Some(url)) => {
            let rev = git_rev(rev, tag, branch, line, err)?;
            Ok(DependencySource::Git { url, rev })
        }
        (None, None) => Err(err(
            line,
            "a dependency needs a path or a git source",
            "write { path = \"lib/greet\" } or { git = \"https://…\" }",
        )),
    }
}

/// Fold the optional `rev`/`tag`/`branch` fields into one [`GitRev`], rejecting a
/// combination of more than one.
fn git_rev(
    rev: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
    line: u32,
    err: &impl Fn(u32, &str, &str) -> Diagnostic,
) -> Result<GitRev, Diagnostic> {
    let mut chosen = Vec::new();
    if let Some(rev) = rev {
        chosen.push(GitRev::Rev(rev));
    }
    if let Some(tag) = tag {
        chosen.push(GitRev::Tag(tag));
    }
    if let Some(branch) = branch {
        chosen.push(GitRev::Branch(branch));
    }
    match chosen.len() {
        0 => Ok(GitRev::Default),
        1 => Ok(chosen
            .into_iter()
            .next()
            .expect("compiler bug: one git rev")),
        _ => Err(err(
            line,
            "a git dependency pins one of rev, tag, or branch",
            "keep just one of rev/tag/branch",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> Manifest {
        parse("doge.toml", source).expect("manifest should parse")
    }

    #[test]
    fn parses_package_and_path_dependency() {
        let m = parse_ok(
            "[package]\nname = \"app\"\nversion = \"0.2.0\"\nentry = \"main.doge\"\n\n[dependencies]\ngreet = { path = \"lib/greet\" }\n",
        );
        assert_eq!(m.name, "app");
        assert_eq!(m.version, "0.2.0");
        assert_eq!(m.entry, "main.doge");
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!(m.dependencies[0].alias, "greet");
        assert_eq!(
            m.dependencies[0].source,
            DependencySource::Path("lib/greet".to_string())
        );
        assert_eq!(m.dependencies[0].line, 7);
    }

    #[test]
    fn version_and_entry_have_defaults() {
        let m = parse_ok("[package]\nname = \"app\"\n");
        assert_eq!(m.version, "0.0.0");
        assert_eq!(m.entry, "main.doge");
        assert!(m.dependencies.is_empty());
    }

    #[test]
    fn parses_git_dependency_with_a_tag() {
        let m = parse_ok(
            "[package]\nname = \"app\"\n\n[dependencies]\ncool = { git = \"https://example.com/cool.git\", tag = \"v1.0.0\" }\n",
        );
        assert_eq!(
            m.dependencies[0].source,
            DependencySource::Git {
                url: "https://example.com/cool.git".to_string(),
                rev: GitRev::Tag("v1.0.0".to_string()),
            }
        );
    }

    #[test]
    fn a_bare_git_dependency_uses_the_default_branch() {
        let m = parse_ok(
            "[package]\nname = \"app\"\n\n[dependencies]\ncool = { git = \"https://example.com/cool.git\" }\n",
        );
        match &m.dependencies[0].source {
            DependencySource::Git { rev, .. } => assert_eq!(*rev, GitRev::Default),
            other => panic!("expected a git dep, got {other:?}"),
        }
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let m = parse_ok("# a manifest\n\n[package]\nname = \"app\" # the name\n");
        assert_eq!(m.name, "app");
    }

    #[test]
    fn a_missing_name_is_an_error() {
        let err = parse("doge.toml", "[package]\nversion = \"1.0.0\"\n").unwrap_err();
        assert_eq!(err.headline, MANIFEST_HEADLINE);
        assert!(err.message.contains("package name"));
    }

    #[test]
    fn path_and_git_together_are_rejected() {
        let err = parse(
            "doge.toml",
            "[package]\nname = \"a\"\n\n[dependencies]\nx = { path = \"p\", git = \"https://g\" }\n",
        )
        .unwrap_err();
        assert!(err.message.contains("not both"));
    }

    #[test]
    fn two_git_revs_are_rejected() {
        let err = parse(
            "doge.toml",
            "[package]\nname = \"a\"\n\n[dependencies]\nx = { git = \"https://g\", tag = \"v1\", branch = \"main\" }\n",
        )
        .unwrap_err();
        assert!(err.message.contains("one of rev"));
    }

    #[test]
    fn an_unknown_section_is_an_error() {
        let err = parse("doge.toml", "[deps]\nx = 1\n").unwrap_err();
        assert!(err.message.contains("[deps]"));
    }

    #[test]
    fn an_unknown_package_key_is_an_error() {
        let err = parse("doge.toml", "[package]\nname = \"a\"\nauthor = \"me\"\n").unwrap_err();
        assert!(err.message.contains("author"));
    }
}
