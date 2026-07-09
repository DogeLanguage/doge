/// A single compile error. The front end stops at the first one (docs/ERRORS.md:
/// "one issue at a time"), so a failed compile yields exactly one `Diagnostic`.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    /// The meme framing line, e.g. `very error. much confuse.`. A more specific
    /// headline is used when one fits (`very tab. much confuse.`).
    pub headline: String,
    /// Path of the source file, shown above the offending line.
    pub path: String,
    /// 1-based line the error points at.
    pub line: u32,
    /// 1-based column the caret sits under.
    pub col: u32,
    /// The offending source line, verbatim (no trailing newline).
    pub source_line: String,
    /// The precise, plain-language explanation printed after the caret.
    pub message: String,
    /// An optional concrete fix, rendered as `such fix: …`.
    pub hint: Option<String>,
}

/// The default meme framing, used unless a more specific headline fits.
pub const DEFAULT_HEADLINE: &str = "very error. much confuse.";

impl Diagnostic {
    /// Build a diagnostic with the default headline.
    pub fn new(
        path: impl Into<String>,
        line: u32,
        col: u32,
        source_line: impl Into<String>,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic {
            headline: DEFAULT_HEADLINE.to_string(),
            path: path.into(),
            line,
            col,
            source_line: source_line.into(),
            message: message.into(),
            hint: None,
        }
    }

    /// Replace the default headline with a specific meme framing.
    pub fn with_headline(mut self, headline: impl Into<String>) -> Diagnostic {
        self.headline = headline.into();
        self
    }

    /// Attach a `such fix: …` hint.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Diagnostic {
        self.hint = Some(hint.into());
        self
    }

    /// Render the diagnostic in the exact docs/ERRORS.md shape:
    ///
    /// ```text
    /// very error. much confuse.
    ///
    ///   examples/hello.doge:4
    ///     bark "hello" + 5
    ///                  ^ cannot + a Str and an Int
    ///
    /// such fix: turn the Int into a Str first, e.g. str(5)
    /// ```
    ///
    /// The code line is indented four spaces; the caret sits under `col`
    /// (1-based), so its leading padding is `4 + (col - 1)` spaces.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.headline);
        out.push_str("\n\n");

        // Location line, indented two spaces.
        out.push_str(&format!("  {}:{}\n", self.path, self.line));

        // The offending source line, indented four spaces.
        out.push_str(&format!("    {}\n", self.source_line));

        // Caret line: four spaces of code indent, then (col - 1) more to reach
        // the offending column, then the caret and the message.
        let caret_pad = 4 + self.col.saturating_sub(1) as usize;
        out.push_str(&" ".repeat(caret_pad));
        out.push_str(&format!("^ {}\n", self.message));

        if let Some(hint) = &self.hint {
            out.push_str(&format!("\nsuch fix: {hint}\n"));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_matches_design_section_7() {
        // The source line is passed verbatim (top-level, no indentation); render
        // adds the 4-space code indent and places the caret under `+` at col 14.
        let diag = Diagnostic::new(
            "examples/hello.doge",
            4,
            14,
            "bark \"hello\" + 5",
            "cannot + a Str and an Int",
        )
        .with_hint("turn the Int into a Str first, e.g. str(5)");

        let expected = "\
very error. much confuse.

  examples/hello.doge:4
    bark \"hello\" + 5
                 ^ cannot + a Str and an Int

such fix: turn the Int into a Str first, e.g. str(5)
";
        assert_eq!(diag.render(), expected);
    }

    #[test]
    fn render_without_hint_omits_fix_block() {
        let diag = Diagnostic::new("f.doge", 1, 1, "wut", "unexpected");
        let rendered = diag.render();
        assert!(!rendered.contains("such fix:"));
        assert!(rendered.ends_with("^ unexpected\n"));
    }
}
