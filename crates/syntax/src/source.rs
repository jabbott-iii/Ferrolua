//! Positions in source text: [`SourceId`] names a source, [`Span`] marks a range of its bytes,
//! and [`LineMap`] turns a byte offset into the line and column a person reads.
//!
//! These are a module of their own because every later part points into source text with them:
//! the lexer's tokens, the trees' nodes, and the diagnostics that every tool reports.

use crate::Error;

/// The length of the longest source, in bytes, that this crate accepts: `u32::MAX - 1`, which
/// is 4,294,967,294.
///
/// Offsets are `u32` values, so every offset in a source, up to the one just past its last
/// byte, must fit in a `u32`. One byte less than that keeps every 1-based line and column within
/// a `u32` too: in a source of `u32::MAX` bytes with no line break, the column just past the last
/// byte would be `2^32`.
pub const MAX_SOURCE_LEN: u32 = u32::MAX - 1;

/// The number that names one source, such as a file or a chunk passed to `load`, among the
/// sources a program reads.
///
/// A [`Span`] holds a source ID instead of a file name, so it stays small. The program that
/// loads the sources gives each one its ID, and keeps the table from IDs to names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(u32);

impl SourceId {
    /// Returns the ID with the number `index`.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the number of this ID.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// A range of bytes in one source, from `start` up to but not including `end`.
///
/// An empty span, with `start == end`, marks the point between two bytes, such as the end of
/// the source. A span means something only when `start <= end` and `end` is at most its
/// source's length; the fields are public, so the code that builds a span keeps that true.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    /// The source that holds the bytes.
    pub source: SourceId,
    /// The offset of the first byte.
    pub start: u32,
    /// The offset just past the last byte.
    pub end: u32,
}

/// The offsets where the lines of one source start, which turn a byte offset into a line and a
/// column.
///
/// A line ends at `\n`, at `\r`, or at either pair, `\r\n` or `\n\r`, as `inclinenumber` in
/// PUC-Rio Lua 5.5.1's `llex.c` counts lines, so each line number is the one PUC-Rio Lua
/// reports. `\n\n` and `\r\r` end two lines. A line break belongs to the line it ends, and the
/// next line starts at the byte after it, so a source that ends with a line break has an empty
/// last line.
///
/// Columns count bytes, not characters, so the source can hold any bytes, valid UTF-8 or not.
/// In `é=1`, the `=` is in column 3, because `é` is two bytes in UTF-8.
///
/// # Examples
///
/// ```
/// use lunaferra_syntax::LineMap;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let lines = LineMap::new(b"x = 1\r\ny = 2\n")?;
/// assert_eq!(lines.line_col(0), (1, 1)); // `x`
/// assert_eq!(lines.line_col(7), (2, 1)); // `y`, after the two bytes of `\r\n`
/// assert_eq!(lines.line_col(13), (3, 1)); // the end, on the empty line after the last `\n`
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineMap {
    /// The offset where each line starts, in order. Line 1 starts at 0, so this is never empty.
    starts: Vec<u32>,
    /// The length of the source, in bytes.
    len: u32,
}

impl LineMap {
    /// Builds the line map of `source`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SourceTooLong`] if `source` is longer than [`MAX_SOURCE_LEN`] bytes.
    pub fn new(source: &[u8]) -> Result<Self, Error> {
        let len = checked_len(source.len(), MAX_SOURCE_LEN)?;
        Ok(Self::counting_from(source, 0, len))
    }

    /// Builds the line map of `source`, whose length `len` [`checked_len`] gave, as
    /// [`LineMap::new`] does, except that the bytes before offset `from` are all on line 1,
    /// whatever they hold.
    ///
    /// PUC-Rio Lua 5.5.1 reads a file whose first line starts with `#` as if that line, up to
    /// its first `\n`, were empty, so a lone `\r` in it ends no line. The lexer builds the line
    /// map of such a file with `from` at that `\n`. `from` past the end counts no line breaks.
    pub(crate) fn counting_from(source: &[u8], from: usize, len: u32) -> Self {
        Self {
            starts: line_starts(source, from, len),
            len,
        }
    }

    /// Returns the line and the column of the byte at `offset`, both counted from 1, with the
    /// column in bytes.
    ///
    /// The source's length, the offset just past its last byte, gives the position where one
    /// more byte would go. An offset past that gives the same position as the length, so an
    /// offset from damaged input can't make this panic.
    #[must_use]
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let offset = offset.min(self.len);
        // The line that holds `offset` is the last one that starts at or before it. Line 1
        // starts at 0, so `line` is at least 1, and the fallbacks below are never taken.
        let line = self.starts.partition_point(|&start| start <= offset);
        let start = self
            .starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        // `MAX_SOURCE_LEN` keeps the number of lines, and `offset - start + 1`, within a `u32`.
        (u32::try_from(line).unwrap_or(u32::MAX), offset - start + 1)
    }
}

/// Returns `len`, a source's length, as a `u32`, or the error for a source longer than `limit`
/// bytes, which must be at most [`MAX_SOURCE_LEN`].
pub(crate) fn checked_len(len: usize, limit: u32) -> Result<u32, Error> {
    match u32::try_from(len) {
        Ok(fits) if fits <= limit => Ok(fits),
        _ => Err(Error::SourceTooLong { len, limit }),
    }
}

/// Returns the offset where each line of `source` starts, from line 1 at offset 0, counting
/// only the line breaks that start at offset `from` or after it. `len` is the length of
/// `source`.
fn line_starts(source: &[u8], from: usize, len: u32) -> Vec<u32> {
    let mut starts = vec![0];
    // The source comes first in the zip, so the offsets stop at the last byte's, which
    // `MAX_SOURCE_LEN` keeps within a `u32`.
    let mut bytes = source.iter().copied().zip(0_u32..).skip(from).peekable();
    while let Some((byte, _)) = bytes.next() {
        if let Some(partner) = partner(byte) {
            // `\r\n` and `\n\r` end one line, because `inclinenumber` takes the other byte of
            // the pair along, but `\n\n` and `\r\r` end two.
            bytes.next_if(|&(next, _)| next == partner);
            starts.push(bytes.peek().map_or(len, |&(_, offset)| offset));
        }
    }
    starts
}

/// Returns the byte that joins `byte` in a two-byte line break, if `byte` ends a line.
const fn partner(byte: u8) -> Option<u8> {
    match byte {
        b'\n' => Some(b'\r'),
        b'\r' => Some(b'\n'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SOURCE_LEN, checked_len};
    use crate::Error;

    // A test through the public API would need a source of `MAX_SOURCE_LEN` bytes to show that
    // the longest one is accepted, and reading 4 GB takes too long, so these check the length
    // alone.

    #[test]
    fn accepts_a_source_of_max_source_len_bytes() {
        let len = usize::try_from(MAX_SOURCE_LEN).unwrap();
        assert_eq!(checked_len(len, MAX_SOURCE_LEN), Ok(MAX_SOURCE_LEN));
    }

    #[test]
    fn accepts_an_empty_source() {
        assert_eq!(checked_len(0, MAX_SOURCE_LEN), Ok(0));
    }

    #[test]
    fn rejects_a_source_one_byte_longer_than_max_source_len() {
        let len = usize::try_from(u32::MAX).unwrap();
        assert_eq!(
            checked_len(len, MAX_SOURCE_LEN),
            Err(Error::SourceTooLong {
                len,
                limit: MAX_SOURCE_LEN
            })
        );
    }

    #[test]
    fn accepts_a_source_at_a_lower_limit_and_rejects_one_byte_more() {
        assert_eq!(checked_len(16, 16), Ok(16));
        assert_eq!(
            checked_len(17, 16),
            Err(Error::SourceTooLong { len: 17, limit: 16 })
        );
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn rejects_a_source_too_long_for_a_u32() {
        let len = usize::try_from(u64::from(u32::MAX) + 1).unwrap();
        assert_eq!(
            checked_len(len, MAX_SOURCE_LEN),
            Err(Error::SourceTooLong {
                len,
                limit: MAX_SOURCE_LEN
            })
        );
    }

    #[test]
    fn rejects_a_source_of_usize_max_bytes() {
        assert_eq!(
            checked_len(usize::MAX, MAX_SOURCE_LEN),
            Err(Error::SourceTooLong {
                len: usize::MAX,
                limit: MAX_SOURCE_LEN
            })
        );
    }
}
