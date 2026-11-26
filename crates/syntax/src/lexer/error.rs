//! The lexer's errors: [`LexError`], with the message that PUC-Rio Lua 5.5.1 gives for the same
//! mistake. A module of its own because the escape reader and the numeral reader name its kinds
//! too.

use std::fmt;

use crate::Span;

/// A mistake in the source that keeps the lexer from reading a token, such as a malformed number
/// or an unfinished string.
///
/// The lexer reports the first mistake in each token, and in each stretch of an f-string's text,
/// then goes on lexing, so a tool sees every token. PUC-Rio Lua stops at the first error, so
/// only the first error of a source is the one it reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    /// What the mistake is.
    pub kind: LexErrorKind,
    /// The bytes the lexer read up to the mistake, from the start of the token, or of the
    /// stretch of f-string text, that holds it. For an f-string that the source ends inside, the
    /// span runs from its `f` to the end.
    pub span: Span,
    /// The line that PUC-Rio's message names: the line the lexer was on when it found the
    /// mistake, which for an unfinished string is the line where the source or the line ends.
    pub line: u32,
    /// What the message says the mistake is near.
    pub near: Near,
}

impl LexError {
    /// Returns the message for this error, as PUC-Rio Lua 5.5.1 writes it after the chunk name
    /// and the line, such as `malformed number near '0x'`.
    ///
    /// The message is bytes, because the text after `near` is a copy of the source, which need
    /// not be valid UTF-8. As in PUC-Rio's message, that text stops at its first zero byte.
    #[must_use]
    pub fn message(&self) -> Vec<u8> {
        let mut message = self.kind.to_string().into_bytes();
        self.near.append_to(&mut message);
        message
    }
}

impl fmt::Display for LexError {
    /// Writes [`LexError::message`], with each sequence of bytes that isn't valid UTF-8 replaced
    /// by U+FFFD, the replacement character.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.message()))
    }
}

impl std::error::Error for LexError {}

/// What a message says an error is near: some of the source's text, or its end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Near {
    /// The text of the token so far, written in quotes, such as `'0x'`. For a short string, this
    /// is its opening quote and the bytes its escapes stand for, as PUC-Rio's lexer keeps them,
    /// followed by the escape that failed. For an f-string, it is the source of the stretch of
    /// text that holds the mistake, from the token that began the stretch.
    Text(Vec<u8>),
    /// The end of the source, written `<eof>`.
    Eof,
}

impl Near {
    /// Appends ` near ` and this text to `message`, as PUC-Rio's `lexerror` writes them: the text
    /// in quotes, up to its first zero byte, or `<eof>`.
    pub(crate) fn append_to(&self, message: &mut Vec<u8>) {
        message.extend_from_slice(b" near ");
        match self {
            Self::Text(text) => {
                message.push(b'\'');
                message.extend(text.iter().copied().take_while(|&byte| byte != 0));
                message.push(b'\'');
            }
            Self::Eof => message.extend_from_slice(b"<eof>"),
        }
    }
}

/// The kinds of [`LexError`], each with the words of its message.
///
/// Every kind but [`LexErrorKind::SingleClosingBrace`] comes from PUC-Rio Lua 5.5.1's lexer,
/// `llex.c`, with the same words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LexErrorKind {
    /// `malformed number`: a numeral that isn't a valid integer or float, such as `0x`, `3x`, or
    /// `1..2`.
    MalformedNumber,
    /// `unfinished string`: a short string or a short f-string with no closing quote before the
    /// end of its line or of the source.
    UnfinishedString,
    /// `unfinished long string (starting at line N)`: a long string or a long f-string with no
    /// closing bracket.
    UnfinishedLongString {
        /// The line of the opening bracket.
        start_line: u32,
    },
    /// `unfinished long comment (starting at line N)`: a long comment with no closing bracket.
    UnfinishedLongComment {
        /// The line of the opening bracket.
        start_line: u32,
    },
    /// `invalid long string delimiter`: `[` and one or more `=` without a second `[`, such as
    /// `[=x`.
    InvalidLongStringDelimiter,
    /// `invalid escape sequence`: a backslash followed by a character that starts no escape,
    /// such as `\q`.
    InvalidEscape,
    /// `hexadecimal digit expected`: `\x` without two hexadecimal digits, or `\u{` without one.
    HexDigitExpected,
    /// `missing '{'`: `\u` not followed by `{`.
    MissingOpeningBrace,
    /// `missing '}'`: `\u{` and its digits not followed by `}`.
    MissingClosingBrace,
    /// `UTF-8 value too large`: `\u{...}` with a value above `7FFFFFFF`.
    Utf8ValueTooLarge,
    /// `decimal escape too large`: `\ddd` with a value above 255.
    DecimalEscapeTooLarge,
    /// `single '}' in f-string`: a `}` in an f-string's text that isn't part of `}}` and ends no
    /// field. Lunaferra's own message, for its own syntax.
    SingleClosingBrace,
}

impl fmt::Display for LexErrorKind {
    /// Writes the words of the message, before `near`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedNumber => f.write_str("malformed number"),
            Self::UnfinishedString => f.write_str("unfinished string"),
            Self::UnfinishedLongString { start_line } => {
                write!(f, "unfinished long string (starting at line {start_line})")
            }
            Self::UnfinishedLongComment { start_line } => {
                write!(f, "unfinished long comment (starting at line {start_line})")
            }
            Self::InvalidLongStringDelimiter => f.write_str("invalid long string delimiter"),
            Self::InvalidEscape => f.write_str("invalid escape sequence"),
            Self::HexDigitExpected => f.write_str("hexadecimal digit expected"),
            Self::MissingOpeningBrace => f.write_str("missing '{'"),
            Self::MissingClosingBrace => f.write_str("missing '}'"),
            Self::Utf8ValueTooLarge => f.write_str("UTF-8 value too large"),
            Self::DecimalEscapeTooLarge => f.write_str("decimal escape too large"),
            Self::SingleClosingBrace => f.write_str("single '}' in f-string"),
        }
    }
}
