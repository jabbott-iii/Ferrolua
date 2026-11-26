//! `lunaferra-syntax` reads Lunaferra source. Today it holds the positions that every part of
//! Lunaferra uses to point into source text, source IDs, byte spans, and line maps; the lexer,
//! which splits a source into tokens; the parser, which reads every statement and expression,
//! with Python's operators and their precedence, goes on after each syntax error, and emits the
//! events of a tree; the two trees built from those events, the lossless syntax tree, which keeps
//! every byte of the source, and the compact tree, which holds only what the compiler needs; and
//! the diagnostic type that every tool reports. The [syntax spec] describes each part, and the
//! [roadmap] the parts still to come.
//!
//! It depends on nothing but the standard library. The compiler will read the compact tree, and
//! the analysis tools the lossless tree.
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
//! - The compact tree: [`CompactTree::build`] builds a [`CompactTree`] from the same tokens and
//!   events, for a source with no error, and indexing it with the ids and lists in [`compact`],
//!   such as [`compact::ExprId`], gives each [`compact::Stat`] and [`compact::Expr`], with names
//!   and literals as values and the span of each part.
//! - A source's errors: [`source_errors`] gives every lexical and syntax error as a
//!   [`SourceError`], in the order PUC-Rio Lua's parser meets them, and [`diagnostics`] gives each
//!   as a [`Diagnostic`], the shape every tool reports a problem in, with its [`Code`],
//!   [`Severity`], [`Label`]s, and [`Fix`]es, each made of [`Edit`]s. A code's [`Tool`] gives its
//!   letter.
//! - [`Error`] is the error the crate's functions return, and [`MAX_SOURCE_LEN`] is the length of
//!   the longest source they accept.
//! - The limits on reading a source: [`Limits`], which [`lex`] takes, sets the longest source a
//!   host reads, and [`MAX_DEPTH`] is the deepest nesting the parser reads.
//!
//! # Example
//!
//! ```
//! use lunaferra_syntax::{Limits, Origin, SourceId, TokenKind, lex};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let source = b"local x = 1\nprint(f\"x is {x}\")\n";
//! let lexed = lex(source, SourceId::new(0), Origin::String, &Limits::default())?;
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
//! # Limits
//!
//! [`lex`] refuses a source longer than its [`Limits`] allow, 16 MB by default, before it reads a
//! byte. The parser reads at most [`MAX_DEPTH`] levels of nesting, PUC-Rio Lua 5.5.1's limit for
//! `load`, and one level more is the error `C stack overflow`, after which it goes on, so no
//! source can make it overflow the call stack. No setting raises that limit.
//!
//! # Errors in the source
//!
//! The lexer never panics, whatever the bytes. A mistake in the source, such as an unfinished
//! string, is a [`LexError`] in the [`Lexed`], not an `Err`: the lexer marks the bytes as a
//! [`TokenKind::Error`] token and goes on, so the tools see the whole source. The first
//! error's message is the one PUC-Rio Lua 5.5.1 gives for the same source, where the syntax is
//! Lua's.
//!
//! # Recovery, and the first error
//!
//! The parser goes on after each syntax error, which comes as an [`Event::Error`]: it puts the
//! tokens it skips in a [`NodeKind::Error`] node, and goes on at the next token that can start or
//! end a statement. So the tools see every error of a file, and its events always make a tree
//! over every token. PUC-Rio Lua stops at a source's first error, so only the first error, lexical
//! or syntactic, is the one it reports; the first that [`source_errors`] gives is that one, with
//! the message and line PUC-Rio Lua 5.5.1 gives where the syntax is Lua's.
//!
//! # The lossless tree
//!
//! [`SyntaxTree::build`] checks that its events make one tree over every token of the source, in
//! order, and returns an [`Error`] if they don't, so the [`SyntaxNode::text`] of every tree's
//! root is its source, byte for byte. The tree is stored as one flat list, so building, walking,
//! printing, and dropping it never recurse, however deeply its nodes nest.
//!
//! # The compact tree
//!
//! [`CompactTree::build`] refuses a source with any lexical or syntax error, events that make no
//! tree, and a node that doesn't hold the parts of its kind, so every node of a [`CompactTree`]
//! has the parts of its kind. It too is stored as flat lists, so building, cloning, and dropping
//! it never recurse. Its enums, such as [`compact::StatKind`], are exhaustive, so a `match` on one
//! needs no `_` arm.
//!
//! [syntax spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/syntax.md
//! [roadmap]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/roadmap.md

pub mod ast;
pub mod compact;
mod diagnostic;
mod error;
mod event;
mod lexer;
mod limits;
mod parser;
mod source;
mod source_error;
mod token;
mod tree;

pub use compact::CompactTree;
pub use diagnostic::{Code, Diagnostic, Edit, Fix, Label, Severity, Tool};
pub use error::Error;
pub use event::{Event, NodeKind};
pub use lexer::{LexError, LexErrorKind, Lexed, Near, Number, Origin, lex, numeral_value};
pub use limits::{Limits, MAX_DEPTH};
pub use parser::{ParseError, ParseErrorKind, parse};
pub use source::{LineMap, MAX_SOURCE_LEN, SourceId, Span};
pub use source_error::{SourceError, diagnostics, source_errors};
pub use token::{Token, TokenKind, Trivia, TriviaKind};
pub use tree::{Children, SyntaxElement, SyntaxNode, SyntaxToken, SyntaxTree};
