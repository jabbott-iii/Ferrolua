//! The parser's output: a list of [`Event`]s, each of which starts a node of a [`NodeKind`], adds
//! a token to the open node, or finishes it.
//!
//! These are a module of their own because two builders read the same events: the lossless tree
//! for the tools, which [`SyntaxTree::build`](crate::SyntaxTree::build) builds, and the compact
//! tree for the compiler. Both see the same program, because both read what the one parser saw
//! (ADR-0009: one parser for the compiler and the tools).

use crate::{Span, TokenKind};

/// One step of a parse: start a node, add a token to the open node, or finish the open node.
///
/// A list of events describes a tree in the order a reader meets its parts. A `Start` opens a
/// node inside the node that is open, a `Token` adds the source's next token to the node that is
/// open, and a `Finish` closes it. For `x` on its own, the events of a tree with a chunk, a
/// block, and the end of the source could be:
///
/// ```text
/// Start(Chunk), Start(Block), Token(Name, 0..1), Finish, Token(Eof, 1..1), Finish
/// ```
///
/// [`SyntaxTree::build`](crate::SyntaxTree::build) says which lists make a tree.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// Opens a node of this kind inside the node that is open, or as the root.
    Start(NodeKind),
    /// Adds the source's next token, of this kind and with these bytes, to the node that is
    /// open.
    Token(TokenKind, Span),
    /// Closes the node that is open.
    Finish,
}

/// What a node of a syntax tree is.
///
/// The kinds follow the grammar of Lua 5.5 (§9 of the manual) and Lunaferra's additions to it.
/// Only the kinds that frame a source are here so far; the parser will add a kind for each
/// statement and expression it learns to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NodeKind {
    /// A whole source, the root of its tree: its block, then the [`TokenKind::Eof`] token, whose
    /// leading trivia holds the spaces and comments after the last statement.
    Chunk,
    /// A block: a sequence of statements, as the body of a chunk, a function, or a loop is.
    Block,
    /// The tokens that the parser skipped to recover from a syntax error, which mark the place
    /// of the error in the tree.
    Error,
}
