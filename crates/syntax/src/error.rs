//! The error type of the crate's functions, kept in a module of its own so that each module
//! that returns it can name it.

use std::fmt;

use crate::MAX_SOURCE_LEN;

/// An error from one of this crate's functions.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The source is longer than [`MAX_SOURCE_LEN`] bytes, the longest that this crate accepts.
    SourceTooLong {
        /// The length of the source, in bytes.
        len: usize,
    },
    /// An event falls outside the root node: a [`Start`](crate::Event::Start) after the root
    /// is finished, a [`Token`](crate::Event::Token) with no node open, or a
    /// [`Finish`](crate::Event::Finish) with no node open.
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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLong { len } => write!(
                f,
                "source of {len} bytes is longer than the limit of {MAX_SOURCE_LEN} bytes"
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
        }
    }
}

impl std::error::Error for Error {}
