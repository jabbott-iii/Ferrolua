//! The shape every Lunaferra tool reports a problem in: [`Diagnostic`], with its [`Code`],
//! [`Severity`], [`Label`]s, and [`Fix`]es.
//!
//! The type lives here, in the crate every tool depends on, so that the parser, the compiler,
//! the type checker, the linter, the formatter, the security scanner, and the package manager
//! report alike, and the command line and the language server print any of them the same way.
//! The [diagnostics spec] is the contract: the codes, their registry, the output formats, and the
//! suppression comments.
//!
//! [diagnostics spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/diagnostics.md

use std::fmt;

use crate::Span;

/// A problem that a tool found in a source: what it is, where, and how to fix it when the fix is
/// certain.
///
/// [`Diagnostic::new`] makes one with no labels, notes, or fixes, which a tool then adds to the
/// public fields. The syntax errors of a source come as diagnostics from
/// [`diagnostics`](crate::diagnostics).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// The stable code that names the kind of problem, such as `P0013`.
    pub code: Code,
    /// How serious the problem is.
    pub severity: Severity,
    /// What the problem is, in one sentence with no trailing period.
    pub message: String,
    /// The source and the bytes of the main problem.
    pub span: Span,
    /// Other places that bear on the problem, each with a short message.
    pub labels: Vec<Label>,
    /// Help and background, each one sentence.
    pub notes: Vec<String>,
    /// Edits to the source that resolve the problem, when one is certain.
    pub fixes: Vec<Fix>,
}

impl Diagnostic {
    /// Returns a diagnostic of `code` and `severity` at `span`, with `message` and no labels,
    /// notes, or fixes.
    #[must_use]
    pub fn new(code: Code, severity: Severity, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            span,
            labels: Vec::new(),
            notes: Vec::new(),
            fixes: Vec::new(),
        }
    }
}

/// How serious a [`Diagnostic`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Severity {
    /// The source is wrong: it doesn't compile, or a check the project turned on fails.
    Error,
    /// The source works, but probably not as meant.
    Warning,
    /// Information that doesn't call for a change.
    Note,
}

impl fmt::Display for Severity {
    /// Writes `error`, `warning`, or `note`, the names the text output and the settings use.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        })
    }
}

/// The tool that a [`Code`] belongs to, which gives the code its letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Tool {
    /// `P`: the lexer's and the parser's syntax errors.
    Parser,
    /// `C`: the compiler's checks, strict mode and the rules of `<const>`.
    Compiler,
    /// `T`: the type checker.
    TypeChecker,
    /// `L`: the linter.
    Linter,
    /// `F`: the formatter, for a file that isn't formatted.
    Formatter,
    /// `S`: static application security testing (SAST), the security scanner.
    Sast,
    /// `M`: the package manager, for manifests and lock files.
    Manifest,
}

impl Tool {
    /// Returns the letter that starts this tool's codes, such as `P` for the parser.
    #[must_use]
    pub const fn letter(self) -> char {
        match self {
            Self::Parser => 'P',
            Self::Compiler => 'C',
            Self::TypeChecker => 'T',
            Self::Linter => 'L',
            Self::Formatter => 'F',
            Self::Sast => 'S',
            Self::Manifest => 'M',
        }
    }
}

/// The stable name of a kind of problem: a tool's letter and four digits, such as `P0013`.
///
/// A code is never renamed, and a removed code's number is never reused, so a setting that names
/// one keeps its meaning across releases. Each code is listed in the registry of the
/// [diagnostics spec](https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/diagnostics.md#registry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Code {
    /// The tool, which gives the letter.
    tool: Tool,
    /// The number, from 1 to 9,999.
    number: u16,
}

impl Code {
    /// The highest number a code can have, so that it fits in four digits.
    pub const MAX_NUMBER: u16 = 9_999;

    /// Returns the code of `tool` with `number`, such as `P0013` for the parser and 13.
    ///
    /// # Panics
    ///
    /// Panics if `number` is 0 or above [`Code::MAX_NUMBER`]. A code is a constant of the tool
    /// that reports it, so a wrong number is a bug in that tool, and a code made in a `const`
    /// item with a wrong number fails to compile.
    #[must_use]
    pub const fn new(tool: Tool, number: u16) -> Self {
        assert!(
            number >= 1 && number <= Self::MAX_NUMBER,
            "a code's number is from 1 to 9,999"
        );
        Self { tool, number }
    }

    /// Returns the tool the code belongs to.
    #[must_use]
    pub const fn tool(self) -> Tool {
        self.tool
    }

    /// Returns the code's number, from 1 to [`Code::MAX_NUMBER`].
    #[must_use]
    pub const fn number(self) -> u16 {
        self.number
    }
}

impl fmt::Display for Code {
    /// Writes the tool's letter and the number in four digits, such as `P0013`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{:04}", self.tool.letter(), self.number)
    }
}

/// Another place that bears on a [`Diagnostic`], such as the `if` that a missing `end` closes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// The place.
    pub span: Span,
    /// What the place has to do with the problem, in a few words.
    pub message: String,
}

/// A change to the source that resolves a [`Diagnostic`], made of one or more [`Edit`]s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    /// What the fix does, such as `replace '~=' with '!='`, for an editor's menu.
    pub message: String,
    /// The edits, which don't overlap.
    pub edits: Vec<Edit>,
}

/// One edit of a [`Fix`]: the bytes at a span replaced with new ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    /// The bytes to replace. An empty span inserts at its start.
    pub span: Span,
    /// The bytes to put in their place, which are bytes like the source they go in, not
    /// necessarily valid UTF-8. Empty text deletes the span.
    pub text: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceId;

    #[test]
    fn writes_each_tools_letter_and_four_digits() {
        let cases = [
            (Tool::Parser, 1, "P0001"),
            (Tool::Compiler, 12, "C0012"),
            (Tool::TypeChecker, 345, "T0345"),
            (Tool::Linter, 9_999, "L9999"),
            (Tool::Formatter, 2, "F0002"),
            (Tool::Sast, 40, "S0040"),
            (Tool::Manifest, 7, "M0007"),
        ];
        for (tool, number, text) in cases {
            let code = Code::new(tool, number);
            assert_eq!(code.to_string(), text);
            assert_eq!(code.tool(), tool);
            assert_eq!(code.number(), number);
        }
    }

    #[test]
    #[should_panic(expected = "a code's number is from 1 to 9,999")]
    fn refuses_a_code_numbered_0() {
        let _ = Code::new(Tool::Parser, 0);
    }

    #[test]
    #[should_panic(expected = "a code's number is from 1 to 9,999")]
    fn refuses_a_code_numbered_above_9_999() {
        let _ = Code::new(Tool::Parser, Code::MAX_NUMBER + 1);
    }

    #[test]
    fn makes_a_diagnostic_with_no_labels_notes_or_fixes() {
        let span = Span {
            source: SourceId::new(3),
            start: 4,
            end: 6,
        };
        let code = Code::new(Tool::Linter, 1);
        let diagnostic = Diagnostic::new(code, Severity::Warning, "unused variable 'x'", span);
        assert_eq!(
            diagnostic,
            Diagnostic {
                code,
                severity: Severity::Warning,
                message: "unused variable 'x'".to_owned(),
                span,
                labels: Vec::new(),
                notes: Vec::new(),
                fixes: Vec::new(),
            }
        );
    }

    #[test]
    fn writes_each_severity_by_its_name_in_the_settings() {
        assert_eq!(Severity::Error.to_string(), "error");
        assert_eq!(Severity::Warning.to_string(), "warning");
        assert_eq!(Severity::Note.to_string(), "note");
    }

    #[test]
    fn orders_codes_by_tool_then_number() {
        let mut codes = [
            Code::new(Tool::Linter, 1),
            Code::new(Tool::Parser, 2),
            Code::new(Tool::Parser, 1),
        ];
        codes.sort();
        let written: Vec<String> = codes.iter().map(ToString::to_string).collect();
        assert_eq!(written, ["P0001", "P0002", "L0001"]);
    }
}
