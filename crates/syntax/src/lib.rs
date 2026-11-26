//! `lunaferra-syntax` reads Lunaferra source. Today it holds the positions that every part of
//! Lunaferra uses to point into source text, source IDs, byte spans, and line maps, and the
//! lexer, which splits a source into tokens. The parser, its two syntax trees, and the
//! diagnostic type that every tool shares join it in milestone M1 of the [roadmap], as the
//! [syntax spec] describes.
//!
//! It depends on nothing but the standard library. The compiler and the analysis tools will read
//! the trees it builds.
//!
//! # The parts
//!
//! - Positions: [`SourceId`] names a source, such as a file, among the sources a program reads.
//!   [`Span`] marks a range of bytes in one source. [`LineMap`] turns a byte offset into a line
//!   and a column, and counts lines the way PUC-Rio Lua 5.5.1's lexer does.
//! - The lexer: [`lex`] splits a source into [`Token`]s, each with a [`TokenKind`] and the
//!   [`Trivia`] around it, and returns them in a [`Lexed`], with each [`LexError`]. [`Origin`]
//!   says whether the source is a file, whose first line may be skipped, or a string.
//!   [`numeral_value`] gives a numeral's [`Number`].
//! - [`Error`] is the error the crate's functions return, and [`MAX_SOURCE_LEN`] is the length of
//!   the longest source they accept.
//!
//! # Example
//!
//! ```
//! use lunaferra_syntax::{Origin, SourceId, TokenKind, lex};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let source = b"local x = 1\nprint(f\"x is {x}\")\n";
//! let lexed = lex(source, SourceId::new(0), Origin::String)?;
//! let print = lexed.tokens()[4];
//! assert_eq!(print.kind, TokenKind::Name);
//! // `print` starts at byte 12, on line 2, in column 1.
//! assert_eq!(lexed.lines().line_col(print.span.start), (2, 1));
//! // The line break before it is the leading trivia of `print`.
//! assert_eq!(lexed.leading(&print).len(), 1);
//! assert!(lexed.errors().is_empty());
//! # Ok(())
//! # }
//! ```
//!
//! # Positions are byte offsets
//!
//! Lua source is bytes, and need not be valid UTF-8, so every position is a byte offset held in
//! a `u32`, and every column counts bytes. [`LineMap::new`] and [`lex`] refuse a source longer
//! than [`MAX_SOURCE_LEN`] bytes, which keeps every offset, line, and column within a `u32`.
//!
//! # Errors in the source
//!
//! The lexer never panics, whatever the bytes. A mistake in the source, such as an unfinished
//! string, is a [`LexError`] in the [`Lexed`], not an `Err`: the lexer marks the bytes as a
//! [`TokenKind::Error`] token and goes on, so the tools see the whole source. The first
//! error's message is the one PUC-Rio Lua 5.5.1 gives for the same source, where the syntax is
//! Lua's.
//!
//! [syntax spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/syntax.md
//! [roadmap]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/roadmap.md

mod error;
mod lexer;
mod source;
mod token;

pub use error::Error;
pub use lexer::{LexError, LexErrorKind, Lexed, Near, Number, Origin, lex, numeral_value};
pub use source::{LineMap, MAX_SOURCE_LEN, SourceId, Span};
pub use token::{Token, TokenKind, Trivia, TriviaKind};
