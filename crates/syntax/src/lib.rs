//! `lunaferra-syntax` reads Lunaferra source. Today it holds the positions that every part of
//! Lunaferra uses to point into source text: source IDs, byte spans, and line maps. The lexer,
//! the parser, its two syntax trees, and the diagnostic type that every tool shares join it in
//! milestone M1 of the [roadmap], as the [syntax spec] describes.
//!
//! It depends on nothing but the standard library. The compiler and the analysis tools will read
//! the trees it builds.
//!
//! # The parts
//!
//! - [`SourceId`] names a source, such as a file, among the sources a program reads.
//! - [`Span`] marks a range of bytes in one source.
//! - [`LineMap`] turns a byte offset into a line and a column, and counts lines the way PUC-Rio
//!   Lua 5.5.1's lexer does.
//! - [`Error`] is the error the crate's functions return, and [`MAX_SOURCE_LEN`] is the length of
//!   the longest source they accept.
//!
//! # Example
//!
//! ```
//! use lunaferra_syntax::{LineMap, SourceId, Span};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let source = b"local x = 1\nprint(x)\n";
//! // The span of `print`, in the source numbered 0.
//! let span = Span { source: SourceId::new(0), start: 12, end: 17 };
//! let lines = LineMap::new(source)?;
//! assert_eq!(lines.line_col(span.start), (2, 1));
//! # Ok(())
//! # }
//! ```
//!
//! # Positions are byte offsets
//!
//! Lua source is bytes, and need not be valid UTF-8, so every position is a byte offset held in
//! a `u32`, and every column counts bytes. [`LineMap::new`] refuses a source longer than
//! [`MAX_SOURCE_LEN`] bytes, which keeps every offset, line, and column within a `u32`.
//!
//! [syntax spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/syntax.md
//! [roadmap]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/roadmap.md

mod error;
mod source;

pub use error::Error;
pub use source::{LineMap, MAX_SOURCE_LEN, SourceId, Span};
