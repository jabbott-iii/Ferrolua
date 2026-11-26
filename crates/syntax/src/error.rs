//! The error type of the crate's functions, kept in a module of its own so that each module
//! that returns it can name it.

use std::fmt;

/// An error from one of this crate's functions.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The source is longer than the limit it was read under:
    /// [`MAX_SOURCE_LEN`](crate::MAX_SOURCE_LEN) bytes, the longest that this crate accepts, or the [`Limits`](crate::Limits) given to
    /// [`lex`](crate::lex).
    SourceTooLong {
        /// The length of the source, in bytes.
        len: usize,
        /// The limit, in bytes.
        limit: u32,
    },
    /// An event falls outside the root node: a [`Start`](crate::Event::Start) after the root
    /// is finished, or a [`Token`](crate::Event::Token), a [`Finish`](crate::Event::Finish), or
    /// an [`Error`](crate::Event::Error) with no node open.
    EventOutsideRoot {
        /// The event's position in the list, counted from 0.
        event: usize,
    },
    /// A [`Token`](crate::Event::Token) event doesn't name the source's next token, by its kind
    /// and span, or comes after the last token.
    TokenOutOfOrder {
        /// The event's position in the list, counted from 0.
        event: usize,
    },
    /// The events end before the tree is whole: with no node at all, with a node still open, or
    /// with tokens of the source that no event names.
    EventsEndEarly,
    /// The source has a lexical error, or the events report a syntax error with an
    /// [`Error`](crate::Event::Error) event, so it has no
    /// [`CompactTree`](crate::CompactTree). [`source_errors`](crate::source_errors) gives each
    /// error.
    SourceHasErrors,
    /// A node doesn't hold the parts of its kind, as the
    /// [syntax spec](https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/syntax.md#lossless-tree)
    /// lists them, such as an [`IfStat`](crate::NodeKind::IfStat) without its condition or an
    /// [`Error`](crate::NodeKind::Error) node, or the root isn't a
    /// [`Chunk`](crate::NodeKind::Chunk).
    MalformedNode {
        /// The position in the list of the node's [`Start`](crate::Event::Start) event, counted
        /// from 0.
        event: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLong { len, limit } => write!(
                f,
                "source of {len} bytes is longer than the limit of {limit} bytes"
            ),
            Self::EventOutsideRoot { event } => {
                write!(f, "event {event} falls outside the root node")
            }
            Self::TokenOutOfOrder { event } => {
                write!(f, "event {event} doesn't name the source's next token")
            }
            Self::EventsEndEarly => write!(
                f,
                "the events end before every node is finished and every token is named"
            ),
            Self::SourceHasErrors => write!(f, "the source has a lexical or syntax error"),
            Self::MalformedNode { event } => write!(
                f,
                "the node that event {event} starts doesn't hold the parts of its kind"
            ),
        }
    }
}

impl std::error::Error for Error {}
