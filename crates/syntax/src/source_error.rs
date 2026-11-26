//! A source's errors, lexical and syntactic, in the order PUC-Rio Lua's parser meets them:
//! [`source_errors`] gives each as a [`SourceError`], whose first is the one `load` reports, and
//! [`diagnostics`] gives each as a [`Diagnostic`], with its `P` code, for the tools.
//!
//! The lexer reads a whole source before the parser starts, but PUC-Rio's lexer reads one token
//! at a time, as its parser asks for it. So a lexical error comes before a syntax error when the
//! parser would reach the token that holds it first. This module puts the two lists together in
//! that order, which is why it is a module of its own: neither the lexer nor the parser knows
//! the other's errors.

use std::fmt;

use crate::diagnostic::{Code, Diagnostic, Edit, Fix, Severity, Tool};
use crate::{Event, LexError, LexErrorKind, Lexed, ParseError, ParseErrorKind, Span, Token};

/// An error in a source: a lexical error from [`lex`](crate::lex), or a syntax error from
/// [`parse`](crate::parse).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceError<'a> {
    /// A mistake that kept the lexer from reading a token.
    Lex(&'a LexError),
    /// A token the parser didn't expect, or a rule of PUC-Rio's parser that a statement breaks.
    Parse(&'a ParseError),
}

impl SourceError<'_> {
    /// Returns the message, as PUC-Rio Lua 5.5.1 writes it after the chunk name and the line, as
    /// [`LexError::message`] and [`ParseError::message`] do.
    #[must_use]
    pub fn message(&self) -> Vec<u8> {
        match self {
            Self::Lex(error) => error.message(),
            Self::Parse(error) => error.message(),
        }
    }

    /// Returns the line that PUC-Rio's message names.
    #[must_use]
    pub fn line(&self) -> u32 {
        match self {
            Self::Lex(error) => error.line,
            Self::Parse(error) => error.line,
        }
    }

    /// Returns the bytes the error is about.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Lex(error) => error.span,
            Self::Parse(error) => error.span,
        }
    }

    /// Returns the error as a diagnostic, as [`LexError::diagnostic`] and
    /// [`ParseError::diagnostic`] do.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::Lex(error) => error.diagnostic(),
            Self::Parse(error) => error.diagnostic(),
        }
    }
}

impl fmt::Display for SourceError<'_> {
    /// Writes [`SourceError::message`], with each sequence of bytes that isn't valid UTF-8
    /// replaced by U+FFFD, the replacement character.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => error.fmt(f),
            Self::Parse(error) => error.fmt(f),
        }
    }
}

/// Returns every error of a source, lexical and syntactic, in the order PUC-Rio Lua's parser
/// would meet them, so the first is the one PUC-Rio Lua 5.5.1's `load` reports.
///
/// `lexed` is what [`lex`](crate::lex) gave for the source, and `events` what
/// [`parse`](crate::parse) gave for it. Each syntax error is met at the token the parser was at
/// when it found it, and each lexical error at the token that holds it, which is the first
/// token that ends after the error starts. A lexical error comes before a syntax error met at
/// the same token, because PUC-Rio's lexer fails while it reads that token, before its parser
/// can look at it. Errors met at the same token otherwise keep their order.
///
/// # Examples
///
/// ```
/// use lunaferra_syntax::{Limits, Origin, SourceId, lex, parse, source_errors};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = b"x = = 1\ny = 0x\nz = = 3\n";
/// let lexed = lex(source, SourceId::new(0), Origin::String, &Limits::default())?;
/// let events = parse(source, &lexed);
/// let errors: Vec<String> = source_errors(&lexed, &events)
///     .iter()
///     .map(|error| format!("input:{}: {error}", error.line()))
///     .collect();
/// assert_eq!(
///     errors,
///     [
///         "input:1: unexpected symbol near '='",
///         "input:2: malformed number near '0x'",
///         "input:3: unexpected symbol near '='",
///     ]
/// );
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn source_errors<'a>(lexed: &'a Lexed, events: &'a [Event]) -> Vec<SourceError<'a>> {
    let tokens = lexed.tokens();
    let mut syntax = Vec::new();
    let mut read = 0_usize;
    for event in events {
        match event {
            Event::Token(..) => read = read.saturating_add(1),
            Event::Error(error) => syntax.push((read, error)),
            Event::Start(_) | Event::Finish => {}
        }
    }
    let mut lexical = lexed
        .errors()
        .iter()
        .map(|error| (token_of(tokens, error), error))
        .peekable();
    let mut syntax = syntax.into_iter().peekable();
    let mut errors = Vec::with_capacity(lexed.errors().len().saturating_add(syntax.len()));
    loop {
        let lexical_first = match (lexical.peek(), syntax.peek()) {
            (Some(&(lexical, _)), Some(&(syntax, _))) => lexical <= syntax,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => return errors,
        };
        if lexical_first {
            errors.extend(lexical.next().map(|(_, error)| SourceError::Lex(error)));
        } else {
            errors.extend(syntax.next().map(|(_, error)| SourceError::Parse(error)));
        }
    }
}

/// Returns a [`Diagnostic`] for every error of a source, lexical and syntactic, in the order of
/// [`source_errors`], so the first is the error PUC-Rio Lua 5.5.1 reports.
///
/// # Examples
///
/// ```
/// use lunaferra_syntax::{Limits, Origin, SourceId, diagnostics, lex, parse};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = b"if x ~= 1 then\n  y = 1\n";
/// let lexed = lex(source, SourceId::new(0), Origin::String, &Limits::default())?;
/// let found = diagnostics(&lexed, &parse(source, &lexed));
/// let codes: Vec<String> = found.iter().map(|found| found.code.to_string()).collect();
/// assert_eq!(codes, ["P0026", "P0014"]);
/// assert_eq!(found[0].message, "'~=' is not an operator: use '!=' to test inequality");
/// assert_eq!(found[0].fixes[0].edits[0].text, b"!=");
/// assert_eq!(
///     found[1].message,
///     "'end' expected (to close 'if' at line 1) near <eof>"
/// );
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn diagnostics(lexed: &Lexed, events: &[Event]) -> Vec<Diagnostic> {
    source_errors(lexed, events)
        .iter()
        .map(SourceError::diagnostic)
        .collect()
}

/// Returns the position of the token that holds `error`, which PUC-Rio's lexer is reading when
/// it finds the mistake: the first token that ends after the error starts. That is the `Eof`
/// token for a long comment that the source ends inside, which is the leading trivia of `Eof`.
pub(crate) fn token_of(tokens: &[Token], error: &LexError) -> usize {
    let found = tokens.partition_point(|token| token.span.end <= error.span.start);
    found.min(tokens.len().saturating_sub(1))
}

impl LexError {
    /// Returns this error as a diagnostic: an error with the kind's `P` code, the message as
    /// [`LexError::message`] gives it, with each sequence of bytes that isn't valid UTF-8
    /// replaced by U+FFFD, and the error's span.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(
            self.kind.code(),
            Severity::Error,
            self.to_string(),
            self.span,
        )
    }
}

impl ParseError {
    /// Returns this error as a diagnostic: an error with the kind's `P` code, the message as
    /// [`ParseError::message`] gives it, with each sequence of bytes that isn't valid UTF-8
    /// replaced by U+FFFD, and the error's span.
    ///
    /// The errors for Lua's `~=` and binary `~` come with a fix that writes the operator the
    /// message names, `!=` or `^`, in its place, which parses wherever the old one stopped the
    /// parse. The error for `..` has none, because `+` binds more tightly than `..` did and joins
    /// only strings, so writing it could change what the expression computes.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(
            self.kind.code(),
            Severity::Error,
            self.to_string(),
            self.span,
        );
        let replacement = match self.kind {
            ParseErrorKind::LuaNotEqual => Some(("~=", "!=")),
            ParseErrorKind::LuaBitwiseXor => Some(("~", "^")),
            _ => None,
        };
        if let Some((old, new)) = replacement {
            diagnostic.fixes.push(Fix {
                message: format!("replace '{old}' with '{new}'"),
                edits: vec![Edit {
                    span: self.span,
                    text: new.as_bytes().to_vec(),
                }],
            });
        }
        diagnostic
    }
}

/// Returns the code of the parser's tool, `P`, with `number`.
const fn p(number: u16) -> Code {
    Code::new(Tool::Parser, number)
}

impl LexErrorKind {
    /// Returns the `P` code of this kind of error, from `P0001` to `P0012`, as the registry of
    /// the diagnostics spec lists them.
    pub(crate) const fn code(self) -> Code {
        match self {
            Self::MalformedNumber => p(1),
            Self::UnfinishedString => p(2),
            Self::UnfinishedLongString { .. } => p(3),
            Self::UnfinishedLongComment { .. } => p(4),
            Self::InvalidLongStringDelimiter => p(5),
            Self::InvalidEscape => p(6),
            Self::HexDigitExpected => p(7),
            Self::MissingOpeningBrace => p(8),
            Self::MissingClosingBrace => p(9),
            Self::Utf8ValueTooLarge => p(10),
            Self::DecimalEscapeTooLarge => p(11),
            Self::SingleClosingBrace => p(12),
        }
    }
}

impl ParseErrorKind {
    /// Returns the `P` code of this kind of error, from `P0013` to `P0028`, as the registry of
    /// the diagnostics spec lists them.
    pub(crate) const fn code(&self) -> Code {
        match self {
            Self::Expected(_) => p(13),
            Self::Unclosed { .. } => p(14),
            Self::UnexpectedSymbol => p(15),
            Self::SyntaxError => p(16),
            Self::EqualOrInExpected => p(17),
            Self::NameOrVarargExpected => p(18),
            Self::FunctionArgumentsExpected => p(19),
            Self::BreakOutsideLoop => p(20),
            Self::VarargOutsideVarargFunction => p(21),
            Self::UnknownAttribute { .. } => p(22),
            Self::MultipleToBeClosed => p(23),
            Self::GlobalToBeClosed => p(24),
            Self::LuaConcatenation => p(25),
            Self::LuaNotEqual => p(26),
            Self::LuaBitwiseXor => p(27),
            Self::NestingTooDeep => p(28),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TokenKind;

    /// One error of each lexical kind, in the order of their codes.
    const LEXICAL: [LexErrorKind; 12] = [
        LexErrorKind::MalformedNumber,
        LexErrorKind::UnfinishedString,
        LexErrorKind::UnfinishedLongString { start_line: 1 },
        LexErrorKind::UnfinishedLongComment { start_line: 1 },
        LexErrorKind::InvalidLongStringDelimiter,
        LexErrorKind::InvalidEscape,
        LexErrorKind::HexDigitExpected,
        LexErrorKind::MissingOpeningBrace,
        LexErrorKind::MissingClosingBrace,
        LexErrorKind::Utf8ValueTooLarge,
        LexErrorKind::DecimalEscapeTooLarge,
        LexErrorKind::SingleClosingBrace,
    ];

    /// One error of each syntax kind, in the order of their codes.
    fn syntax() -> [ParseErrorKind; 16] {
        [
            ParseErrorKind::Expected(TokenKind::End),
            ParseErrorKind::Unclosed {
                expected: TokenKind::End,
                opener: TokenKind::If,
                line: 1,
            },
            ParseErrorKind::UnexpectedSymbol,
            ParseErrorKind::SyntaxError,
            ParseErrorKind::EqualOrInExpected,
            ParseErrorKind::NameOrVarargExpected,
            ParseErrorKind::FunctionArgumentsExpected,
            ParseErrorKind::BreakOutsideLoop,
            ParseErrorKind::VarargOutsideVarargFunction,
            ParseErrorKind::UnknownAttribute {
                name: b"x".to_vec(),
            },
            ParseErrorKind::MultipleToBeClosed,
            ParseErrorKind::GlobalToBeClosed,
            ParseErrorKind::LuaConcatenation,
            ParseErrorKind::LuaNotEqual,
            ParseErrorKind::LuaBitwiseXor,
            ParseErrorKind::NestingTooDeep,
        ]
    }

    /// Returns the codes of every kind of lexical and syntax error, in the order of the kinds.
    fn codes() -> Vec<String> {
        let lexical = LEXICAL.iter().map(|kind| kind.code());
        let syntax = syntax().into_iter().map(|kind| kind.code());
        lexical.chain(syntax).map(|code| code.to_string()).collect()
    }

    #[test]
    fn numbers_the_kinds_from_p0001_in_order() {
        let expected: Vec<String> = (1..=28).map(|number| format!("P{number:04}")).collect();
        assert_eq!(codes(), expected);
    }

    #[test]
    fn lists_every_code_in_the_registry_of_the_diagnostics_spec() {
        let spec = include_str!("../../../docs/specs/diagnostics.md");
        let registry: Vec<String> = spec
            .lines()
            .filter_map(|line| line.strip_prefix("| `P"))
            .filter_map(|rest| rest.split_once('`'))
            // The table of prefixes has a row for `P` too, with no digits.
            .filter(|(digits, _)| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .map(|(digits, _)| format!("P{digits}"))
            .collect();
        assert_eq!(registry, codes());
    }
}
