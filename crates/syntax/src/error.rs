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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLong { len } => write!(
                f,
                "source of {len} bytes is longer than the limit of {MAX_SOURCE_LEN} bytes"
            ),
        }
    }
}

impl std::error::Error for Error {}
