//! `lunaferra-syntax` reads Lunaferra source. Today it holds the positions that every part of
//! Lunaferra uses to point into source text, source IDs, byte spans, and line maps; the lexer,
//! which splits a source into tokens; the parser, which reads every statement and expression,
//! with Python's operators and their precedence, and emits the events of a tree; and the lossless
//! syntax tree, which keeps every byte of the source and is built from those events. Error
//! recovery, the compact tree for the compiler, and the diagnostic type that every tool shares
//! join it in milestone M1 of the [roadmap], as the [syntax spec] describes.
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
//! - The parser: [`parse`] reads a source's tokens and returns its [`Event`]s, each of which
//!   starts a node of a [`NodeKind`], adds a token to it, finishes it, or reports a
//!   [`ParseError`], with a [`ParseErrorKind`] and the message PUC-Rio Lua 5.5.1 gives where the
//!   syntax is Lua's.
//! - The lossless tree: [`SyntaxTree::build`] builds a [`SyntaxTree`] from a source's tokens and
//!   its events, and [`SyntaxNode`], [`SyntaxToken`], and [`SyntaxElement`] read it. The typed
//!   views in [`ast`], such as [`ast::IfStat`], give each kind of node named accessors, and
//!   [`ast::Stat`] and [`ast::Expr`] view every statement and every expression.
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
//! # The first error
//!
//! The parser stops at a source's first error, as PUC-Rio Lua does, and still emits the events
//! of a tree over every token, with the rest of the source in a [`NodeKind::Error`] node. Its
//! error is an [`Event::Error`] when it comes before the source's first lexical error, and
//! otherwise the first of [`Lexed::errors`] is the source's first error. Recovery, which goes on
//! after each error, joins it in M1.6.
//!
//! # The lossless tree
//!
//! [`SyntaxTree::build`] checks that its events make one tree over every token of the source, in
//! order, and returns an [`Error`] if they don't, so the [`SyntaxNode::text`] of every tree's
//! root is its source, byte for byte. The tree is stored as one flat list, so building, walking,
//! printing, and dropping it never recurse, however deeply its nodes nest.
//!
//! [syntax spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/syntax.md
//! [roadmap]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/roadmap.md

pub mod ast;
mod error;
mod event;
mod lexer;
mod parser;
mod source;
mod token;
mod tree;

pub use error::Error;
pub use event::{Event, NodeKind};
pub use lexer::{LexError, LexErrorKind, Lexed, Near, Number, Origin, lex, numeral_value};
pub use parser::{ParseError, ParseErrorKind, parse};
pub use source::{LineMap, MAX_SOURCE_LEN, SourceId, Span};
pub use token::{Token, TokenKind, Trivia, TriviaKind};
pub use tree::{Children, SyntaxElement, SyntaxNode, SyntaxToken, SyntaxTree};
