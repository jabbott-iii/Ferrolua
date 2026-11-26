//! The limits that keep reading a source bounded: [`Limits`], which a host sets, and
//! [`MAX_DEPTH`], which no host can change.
//!
//! These are a module of their own because the lexer and the parser both read them, and the
//! embedding API and the tools set them from their own options.

use crate::MAX_SOURCE_LEN;

/// The most levels of nesting the parser reads, counted where PUC-Rio Lua 5.5.1's `lparser.c`
/// calls `enterlevel`: once for each statement and each `subexpr`, once more for each target of an
/// assignment after the first and for each name a `global` declaration gives a value, and, for
/// Lunaferra's own syntax, once for the value after each conditional expression's `else`, which
/// may be another conditional expression.
///
/// PUC-Rio stops at 200 nested C calls, and two of those are already in use when a script's
/// `load` reads a chunk, so 197 levels parse there and the 198th gives `C stack overflow`.
/// PUC-Rio allows one level more for a file that `lua` runs, and one fewer for each C call, such
/// as `pcall`, around `load`; Lunaferra allows 197 everywhere. No host can raise it, because the
/// parser calls itself for each level, and too deep a call stack would end the process.
pub const MAX_DEPTH: u32 = 197;

/// The limits a host sets on the sources it reads, which [`lex`](crate::lex) checks.
///
/// [`Limits::default`] gives the limits of a sandbox, and [`Limits::trusted`] the limits for
/// sources a host trusts. The nesting depth has no setting: it is always [`MAX_DEPTH`].
///
/// # Examples
///
/// ```
/// use lunaferra_syntax::{Error, Limits, Origin, SourceId, lex};
///
/// let mut limits = Limits::default();
/// limits.source_len = 8;
/// assert!(lex(b"x = 1", SourceId::new(0), Origin::String, &limits).is_ok());
/// assert_eq!(
///     lex(b"x = 12345", SourceId::new(0), Origin::String, &limits).err(),
///     Some(Error::SourceTooLong { len: 9, limit: 8 })
/// );
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Limits {
    /// The length of the longest source, in bytes. A longer one is refused before it is read.
    /// Above [`MAX_SOURCE_LEN`], it is [`MAX_SOURCE_LEN`].
    pub source_len: u32,
}

impl Limits {
    /// Returns the limits for sources a host trusts: a source of up to [`MAX_SOURCE_LEN`]
    /// bytes, the longest this crate can read.
    #[must_use]
    pub const fn trusted() -> Self {
        Self {
            source_len: MAX_SOURCE_LEN,
        }
    }
}

impl Default for Limits {
    /// Returns the limits of a sandbox: a source of up to 16 MB, 16,777,216 bytes.
    fn default() -> Self {
        Self {
            source_len: 16 * 1024 * 1024,
        }
    }
}
