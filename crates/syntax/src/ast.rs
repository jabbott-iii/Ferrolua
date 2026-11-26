//! Typed views of the lossless tree, one for each [`NodeKind`]: an abstract syntax tree (AST)
//! that reads a [`SyntaxNode`] through named accessors, such as [`Chunk::block`].
//!
//! A view is a [`SyntaxNode`] of one kind, so it costs nothing to make, and [`AstNode::cast`]
//! makes one only from a node of its kind. Its accessors look among the node's children for each
//! part, and return `None` for a part that isn't there, which happens in a tree built for a
//! source with syntax errors. The views are a module of their own because there will be one for
//! each statement and expression the parser reads.
//!
//! # Examples
//!
//! ```
//! use lunaferra_syntax::ast::{AstNode, Block, Chunk};
//! use lunaferra_syntax::{Event, NodeKind, Origin, SourceId, SyntaxTree, TokenKind, lex};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let lexed = lex(b"", SourceId::new(0), Origin::String)?;
//! let [eof] = lexed.tokens() else {
//!     return Err("an empty source has only its end".into());
//! };
//! let events = [
//!     Event::Start(NodeKind::Chunk),
//!     Event::Start(NodeKind::Block),
//!     Event::Finish,
//!     Event::Token(eof.kind, eof.span),
//!     Event::Finish,
//! ];
//! let tree = SyntaxTree::build(b"", &lexed, &events)?;
//!
//! let chunk = Chunk::cast(tree.root()).ok_or("the root is a chunk")?;
//! assert!(chunk.block().is_some());
//! assert_eq!(chunk.eof().map(|token| token.kind()), Some(TokenKind::Eof));
//! // The root isn't a block, so it has no block view.
//! assert_eq!(Block::cast(tree.root()), None);
//! # Ok(())
//! # }
//! ```

use crate::{NodeKind, SyntaxNode, SyntaxToken, TokenKind};

/// A typed view of the [`SyntaxNode`]s of one kind.
pub trait AstNode<'t>: Sized {
    /// Returns the view of `node`, or `None` if `node` isn't of this view's kind.
    fn cast(node: SyntaxNode<'t>) -> Option<Self>;

    /// Returns the node that this view reads.
    fn syntax(&self) -> SyntaxNode<'t>;
}

/// Defines the view `$name` of the nodes of kind `NodeKind::$kind`.
macro_rules! view {
    ($(#[$doc:meta])* $name:ident, $kind:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name<'t>(SyntaxNode<'t>);

        impl<'t> AstNode<'t> for $name<'t> {
            fn cast(node: SyntaxNode<'t>) -> Option<Self> {
                (node.kind() == NodeKind::$kind).then_some(Self(node))
            }

            fn syntax(&self) -> SyntaxNode<'t> {
                self.0
            }
        }
    };
}

view! {
    /// A whole source, a node of kind [`NodeKind::Chunk`]: its block, then the end of the
    /// source.
    Chunk, Chunk
}

impl<'t> Chunk<'t> {
    /// Returns the chunk's block, the statements of the source.
    #[must_use]
    pub fn block(&self) -> Option<Block<'t>> {
        self.0.child()
    }

    /// Returns the chunk's [`TokenKind::Eof`] token, whose leading trivia holds what follows
    /// the last statement.
    #[must_use]
    pub fn eof(&self) -> Option<SyntaxToken<'t>> {
        self.0.child_token(TokenKind::Eof)
    }
}

view! {
    /// A block, a node of kind [`NodeKind::Block`]: a sequence of statements.
    Block, Block
}

view! {
    /// The tokens that the parser skipped to recover from a syntax error, a node of kind
    /// [`NodeKind::Error`].
    ErrorNode, Error
}
