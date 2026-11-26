//! Typed views of the lossless tree, one for each [`NodeKind`]: an abstract syntax tree (AST)
//! that reads a [`SyntaxNode`] through named accessors, such as [`IfStat::condition`].
//!
//! A view is a [`SyntaxNode`] of one kind, so it costs nothing to make, and [`AstNode::cast`]
//! makes one only from a node of its kind. [`Stat`] and [`Expr`] are views of every statement
//! and of every expression, with one variant for each kind. A view's accessors look among its
//! node's children for each part, and return `None` for a part that isn't there, which happens
//! in a tree built for a source with syntax errors. The views are a module of their own, with
//! the statements and the expressions in modules of theirs, because there is one for each kind
//! of node the parser makes.
//!
//! # Examples
//!
//! ```
//! use lunaferra_syntax::ast::{AstNode, Block, Chunk, Expr, Stat};
//! use lunaferra_syntax::{Limits, Origin, SourceId, SyntaxTree, TokenKind, lex, parse};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let source = b"if ready then start(1) end";
//! let lexed = lex(source, SourceId::new(0), Origin::String, &Limits::default())?;
//! let tree = SyntaxTree::build(source, &lexed, &parse(source, &lexed))?;
//!
//! let chunk = Chunk::cast(tree.root()).ok_or("the root is a chunk")?;
//! assert_eq!(chunk.eof().map(|token| token.kind()), Some(TokenKind::Eof));
//! let block = chunk.block().ok_or("the chunk has a block")?;
//! let Some(Stat::If(stat)) = block.statements().next() else {
//!     return Err("the first statement is an `if`".into());
//! };
//! let Some(Expr::Name(condition)) = stat.condition() else {
//!     return Err("the condition is a name".into());
//! };
//! assert_eq!(condition.name().map(|token| token.text()), Some(&b"ready"[..]));
//! // The root isn't a block, so it has no block view.
//! assert_eq!(Block::cast(tree.root()), None);
//! # Ok(())
//! # }
//! ```

pub use expression::{
    Args, BinaryExpr, CallExpr, CompareExpr, CompareOp, ConditionalExpr, Expr, ExprList,
    FStringExpr, FStringField, FStringPart, FieldExpr, FormatSpec, FunctionExpr, IndexExpr,
    IndexField, ListExpr, LiteralExpr, MethodCallExpr, NameExpr, NamedField, ParenExpr,
    PositionalField, TableExpr, TableField, TupleExpr, UnaryExpr, VarargExpr,
};
pub use statement::{
    AssignStat, AttName, AttNameList, Attrib, BreakStat, CallStat, DoStat, ElseClause,
    ElseifClause, EmptyStat, FuncBody, FuncName, FunctionStat, GenericForStat, GlobalFunctionStat,
    GlobalStat, GotoStat, IfStat, LabelStat, LocalFunctionStat, LocalStat, NameList, NonlocalStat,
    NumericForStat, Param, ParamList, RepeatStat, ReturnStat, Stat, VarargParam, WhileStat,
};

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

/// Defines `$name`, the view of the nodes of every kind that one of its variants' views reads,
/// with one variant for each.
macro_rules! union {
    ($(#[$doc:meta])* $name:ident { $($(#[$variant_doc:meta])* $variant:ident($view:ident)),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum $name<'t> {
            $($(#[$variant_doc])* $variant($view<'t>)),+
        }

        impl<'t> AstNode<'t> for $name<'t> {
            fn cast(node: SyntaxNode<'t>) -> Option<Self> {
                $(
                    if let Some(view) = $view::cast(node) {
                        return Some(Self::$variant(view));
                    }
                )+
                None
            }

            fn syntax(&self) -> SyntaxNode<'t> {
                match self {
                    $(Self::$variant(view) => view.syntax()),+
                }
            }
        }
    };
}

mod expression;
mod statement;

/// Returns the first child of `node` that is a node of view `N`'s kind and comes after its first
/// token of `kind`.
fn after<'t, N: AstNode<'t>>(node: SyntaxNode<'t>, kind: TokenKind) -> Option<N> {
    node.children()
        .skip_while(|&child| child.into_token().is_none_or(|token| token.kind() != kind))
        .find_map(|child| child.into_node().and_then(N::cast))
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

impl<'t> Block<'t> {
    /// Returns the block's statements, in order. Error nodes, which hold what the parser couldn't
    /// read, aren't statements, so they are left out.
    pub fn statements(&self) -> impl Iterator<Item = Stat<'t>> + use<'t> {
        self.0.children_of()
    }
}

view! {
    /// A part of the source that the parser couldn't read, a node of kind [`NodeKind::Error`]:
    /// the tokens it skipped after a syntax error, or a statement that the error left without a
    /// kind.
    ErrorNode, Error
}
