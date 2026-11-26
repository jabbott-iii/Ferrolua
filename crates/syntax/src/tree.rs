//! The lossless syntax tree: [`SyntaxTree`], which [`SyntaxTree::build`] builds from a source's
//! tokens and the parser's [`Event`]s, and the handles that read it, [`SyntaxNode`],
//! [`SyntaxToken`], and [`SyntaxElement`].
//!
//! The tree keeps every byte of its source: each token holds its leading and trailing trivia, so
//! writing out the tokens of the root in order gives back the source. The formatter, the analysis
//! crates, and the language server read it, so it is a module of its own, apart from the parser
//! that produces the events.
//!
//! The tree is stored as one flat list, with each node followed by everything inside it, so
//! building it, walking it, printing it, and dropping it never recurse: a tree nested 100,000
//! levels deep can't overflow the call stack.

use std::fmt;
use std::ptr;

use crate::ast::AstNode;
use crate::lexer::index;
use crate::{Error, Event, Lexed, NodeKind, Span, Token, TokenKind, Trivia};

/// A lossless syntax tree: nodes that each hold their children, nodes and tokens, in source
/// order, with every token's trivia, so the tree gives back its source byte for byte.
///
/// [`SyntaxTree::build`] builds one, and [`SyntaxTree::root`] reads it. The typed views of
/// [`ast`](crate::ast) give each kind of node named accessors.
///
/// # Examples
///
/// ```
/// use lunaferra_syntax::ast::{AstNode, Chunk};
/// use lunaferra_syntax::{Limits, Event, NodeKind, Origin, SourceId, SyntaxTree, lex};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = b"-- a comment\nx\n";
/// let lexed = lex(source, SourceId::new(0), Origin::String, &Limits::default())?;
/// let [name, eof] = lexed.tokens() else {
///     return Err("`x` is one token, then the end".into());
/// };
/// let events = [
///     Event::Start(NodeKind::Chunk),
///     Event::Start(NodeKind::Block),
///     Event::Token(name.kind, name.span),
///     Event::Finish,
///     Event::Token(eof.kind, eof.span),
///     Event::Finish,
/// ];
/// let tree = SyntaxTree::build(source, &lexed, &events)?;
///
/// // The tree gives back the source, comment and line breaks included.
/// assert_eq!(tree.root().text(), source);
/// // The block holds `x` and the comment before it. The last line break is the leading trivia
/// // of the end of the source, which follows the block.
/// let chunk = Chunk::cast(tree.root()).ok_or("the root is a chunk")?;
/// let block = chunk.block().ok_or("the chunk has a block")?;
/// assert_eq!(block.syntax().text(), b"-- a comment\nx");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxTree {
    /// The source's bytes.
    source: Box<[u8]>,
    /// The source's trivia, which each token's `leading` and `trailing` ranges index.
    trivia: Vec<Trivia>,
    /// Every node and token, each node followed by everything inside it.
    elements: Vec<Element>,
    /// The root, the first element, kept here as well so that [`SyntaxTree::root`] needs no
    /// lookup that could fail.
    root: NodeData,
}

/// What the tree stores for one node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NodeData {
    /// What the node is.
    kind: NodeKind,
    /// The position in the element list just past the node's last descendant.
    end: usize,
}

/// One entry of the tree's element list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Element {
    /// A node, whose descendants follow it.
    Node(NodeData),
    /// A token.
    Token(Token),
}

impl SyntaxTree {
    /// Builds the lossless tree of `source`, whose tokens and trivia are `lexed`, from `events`,
    /// the parser's events for it.
    ///
    /// `lexed` must be what [`lex`](crate::lex) gave for `source`. With any other source, the
    /// tree's text means nothing, though no method panics.
    ///
    /// The events make a tree when the first opens the root with a
    /// [`Start`](Event::Start), every later one falls inside the root, and each
    /// [`Finish`](Event::Finish) closes the node opened last that is still open. An
    /// [`Error`](Event::Error) event adds nothing to the tree. The
    /// [`Token`](Event::Token) events must name each token of `lexed` once, in order, by its kind
    /// and span, up to and including the [`TokenKind::Eof`] token, and the last event must close
    /// the root. Building copies the source and its trivia, so it takes time in proportion to the
    /// source's length and the number of events, and it never recurses.
    ///
    /// # Errors
    ///
    /// - [`Error::EventOutsideRoot`] for a `Start` after the root is closed, or a `Token`, a
    ///   `Finish`, or an `Error` with no node open.
    /// - [`Error::TokenOutOfOrder`] for a `Token` event that doesn't name the next token of
    ///   `lexed`, or comes after its last one.
    /// - [`Error::EventsEndEarly`] if the events end with no node at all, with a node still open,
    ///   or before every token is named.
    pub fn build(source: &[u8], lexed: &Lexed, events: &[Event]) -> Result<Self, Error> {
        let tokens = lexed.tokens();
        // A `Finish` adds no element, so the events bound the number of elements.
        let mut elements = Vec::with_capacity(events.len());
        // The position and kind of each open node, the innermost last.
        let mut open: Vec<(usize, NodeKind)> = Vec::new();
        let mut named = 0;
        for (event, step) in events.iter().enumerate() {
            let outside = Error::EventOutsideRoot { event };
            match *step {
                Event::Start(kind) => {
                    if open.is_empty() && !elements.is_empty() {
                        return Err(outside);
                    }
                    // The node's end is a placeholder until its `Finish`.
                    open.push((elements.len(), kind));
                    elements.push(Element::Node(NodeData { kind, end: 0 }));
                }
                Event::Token(kind, span) => {
                    if open.is_empty() {
                        return Err(outside);
                    }
                    let token = tokens
                        .get(named)
                        .filter(|token| token.kind == kind && token.span == span)
                        .ok_or(Error::TokenOutOfOrder { event })?;
                    elements.push(Element::Token(*token));
                    named += 1;
                }
                Event::Finish => {
                    let (node, kind) = open.pop().ok_or(outside)?;
                    let end = elements.len();
                    // `node` is the position where this loop pushed the node, and the list
                    // only grows, so the position is inside it.
                    elements[node] = Element::Node(NodeData { kind, end });
                }
                // An error adds nothing to the tree, whose error node marks the place.
                Event::Error(_) => {
                    if open.is_empty() {
                        return Err(outside);
                    }
                }
            }
        }
        let Some(&Element::Node(root)) = elements.first() else {
            return Err(Error::EventsEndEarly);
        };
        if !open.is_empty() || named != tokens.len() {
            return Err(Error::EventsEndEarly);
        }
        Ok(Self {
            source: source.into(),
            trivia: lexed.trivia().to_vec(),
            elements,
            root,
        })
    }

    /// Returns the root node, which holds every other node and every token.
    #[must_use]
    pub fn root(&self) -> SyntaxNode<'_> {
        SyntaxNode {
            tree: self,
            position: 0,
            data: self.root,
        }
    }

    /// Returns the source's bytes, which the spans of its tokens and trivia index.
    #[must_use]
    pub fn source(&self) -> &[u8] {
        &self.source
    }

    /// Returns the bytes of `span` in the source, or none if it isn't inside the source.
    fn bytes(&self, span: Span) -> &[u8] {
        self.source
            .get(index(span.start)..index(span.end))
            .unwrap_or(&[])
    }

    /// Returns the trivia in `range`, one of a token's trivia ranges.
    fn trivia_in(&self, (start, end): (u32, u32)) -> &[Trivia] {
        self.trivia.get(index(start)..index(end)).unwrap_or(&[])
    }

    /// Appends `token`'s leading trivia, its bytes, and its trailing trivia to `out`.
    fn write_token(&self, token: &Token, out: &mut Vec<u8>) {
        for trivia in self.trivia_in(token.leading) {
            out.extend_from_slice(self.bytes(trivia.span));
        }
        out.extend_from_slice(self.bytes(token.span));
        for trivia in self.trivia_in(token.trailing) {
            out.extend_from_slice(self.bytes(trivia.span));
        }
    }
}

/// A node of a [`SyntaxTree`]: its kind, and its children, nodes and tokens, in source order.
///
/// A node is a handle that borrows its tree, so it is cheap to copy. Two nodes are equal when
/// they are the same node of the same tree.
#[derive(Clone, Copy)]
pub struct SyntaxNode<'t> {
    /// The tree that holds the node.
    tree: &'t SyntaxTree,
    /// The node's position in the tree's element list.
    position: usize,
    /// The node's kind and the end of its descendants.
    data: NodeData,
}

impl<'t> SyntaxNode<'t> {
    /// Returns what the node is.
    #[must_use]
    pub fn kind(&self) -> NodeKind {
        self.data.kind
    }

    /// Returns the node's children, the nodes and tokens directly inside it, in source order.
    #[must_use]
    pub fn children(&self) -> Children<'t> {
        Children {
            node: *self,
            position: self.position + 1,
        }
    }

    /// Returns the node's text: the bytes of every token inside it, each with its leading and
    /// trailing trivia, in order. The root's text is the whole source.
    #[must_use]
    pub fn text(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let inside = self.tree.elements.get(self.position..self.data.end);
        for element in inside.unwrap_or(&[]) {
            if let Element::Token(token) = element {
                self.tree.write_token(token, &mut out);
            }
        }
        out
    }

    /// Returns the first child that is a node of view `N`'s kind, as that view.
    pub(crate) fn child<N: AstNode<'t>>(&self) -> Option<N> {
        self.children()
            .find_map(|child| child.into_node().and_then(N::cast))
    }

    /// Returns the first child that is a token of `kind`.
    pub(crate) fn child_token(&self, kind: TokenKind) -> Option<SyntaxToken<'t>> {
        self.child_tokens(kind).next()
    }

    /// Returns the children that are nodes of view `N`'s kinds, as that view, in order.
    pub(crate) fn children_of<N: AstNode<'t>>(&self) -> impl Iterator<Item = N> + use<'t, N> {
        self.children()
            .filter_map(|child| child.into_node().and_then(N::cast))
    }

    /// Returns the children that are tokens of `kind`, in order.
    pub(crate) fn child_tokens(
        &self,
        kind: TokenKind,
    ) -> impl Iterator<Item = SyntaxToken<'t>> + use<'t> {
        self.children()
            .filter_map(SyntaxElement::into_token)
            .filter(move |token| token.kind() == kind)
    }

    /// Returns the first child that is a token, of any kind.
    pub(crate) fn first_token(&self) -> Option<SyntaxToken<'t>> {
        self.children().find_map(SyntaxElement::into_token)
    }
}

impl PartialEq for SyntaxNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.tree, other.tree) && self.position == other.position
    }
}

impl Eq for SyntaxNode<'_> {}

impl fmt::Debug for SyntaxNode<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxNode")
            .field("kind", &self.data.kind)
            .field("position", &self.position)
            .finish()
    }
}

/// A token of a [`SyntaxTree`], with its trivia.
///
/// A token is a handle that borrows its tree, so it is cheap to copy. Two tokens are equal when
/// they are the same token of the same tree.
#[derive(Clone, Copy)]
pub struct SyntaxToken<'t> {
    /// The tree that holds the token.
    tree: &'t SyntaxTree,
    /// The token, with its trivia ranges in the tree.
    token: Token,
}

impl<'t> SyntaxToken<'t> {
    /// Returns what the token is.
    #[must_use]
    pub fn kind(&self) -> TokenKind {
        self.token.kind
    }

    /// Returns the span of the token's own bytes, without its trivia.
    #[must_use]
    pub fn span(&self) -> Span {
        self.token.span
    }

    /// Returns the token's own bytes, without its trivia.
    #[must_use]
    pub fn text(&self) -> &'t [u8] {
        self.tree.bytes(self.token.span)
    }

    /// Returns the trivia before the token, as [`Lexed::leading`] gives it.
    #[must_use]
    pub fn leading(&self) -> &'t [Trivia] {
        self.tree.trivia_in(self.token.leading)
    }

    /// Returns the trivia after the token on its line, as [`Lexed::trailing`] gives it.
    #[must_use]
    pub fn trailing(&self) -> &'t [Trivia] {
        self.tree.trivia_in(self.token.trailing)
    }
}

impl PartialEq for SyntaxToken<'_> {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.tree, other.tree) && self.token == other.token
    }
}

impl Eq for SyntaxToken<'_> {}

impl fmt::Debug for SyntaxToken<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxToken")
            .field("kind", &self.token.kind)
            .field("span", &self.token.span)
            .finish()
    }
}

/// A child of a [`SyntaxNode`]: a node or a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxElement<'t> {
    /// A node.
    Node(SyntaxNode<'t>),
    /// A token.
    Token(SyntaxToken<'t>),
}

impl<'t> SyntaxElement<'t> {
    /// Returns the node, if the element is one.
    #[must_use]
    pub fn into_node(self) -> Option<SyntaxNode<'t>> {
        match self {
            Self::Node(node) => Some(node),
            Self::Token(_) => None,
        }
    }

    /// Returns the token, if the element is one.
    #[must_use]
    pub fn into_token(self) -> Option<SyntaxToken<'t>> {
        match self {
            Self::Node(_) => None,
            Self::Token(token) => Some(token),
        }
    }
}

/// The children of a [`SyntaxNode`], in source order, which [`SyntaxNode::children`] returns.
///
/// It steps over each child node's descendants without visiting them, so each step takes the
/// same time however large the child is.
#[derive(Clone, Debug)]
pub struct Children<'t> {
    /// The node whose children these are.
    node: SyntaxNode<'t>,
    /// The position in the tree's element list of the next child.
    position: usize,
}

impl<'t> Iterator for Children<'t> {
    type Item = SyntaxElement<'t>;

    fn next(&mut self) -> Option<Self::Item> {
        let tree = self.node.tree;
        let position = self.position;
        let element = *tree
            .elements
            .get(position)
            .filter(|_| position < self.node.data.end)?;
        Some(match element {
            Element::Node(data) => {
                self.position = data.end;
                SyntaxElement::Node(SyntaxNode {
                    tree,
                    position,
                    data,
                })
            }
            Element::Token(token) => {
                self.position = position + 1;
                SyntaxElement::Token(SyntaxToken { tree, token })
            }
        })
    }
}
