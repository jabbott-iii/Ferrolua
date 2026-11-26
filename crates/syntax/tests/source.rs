//! Tests of source IDs and line maps through the public API: the line and column of each offset,
//! for every line ending that PUC-Rio Lua 5.5.1's lexer counts, and for sources that aren't valid
//! UTF-8.
//!
//! `inclinenumber`, in PUC-Rio's `llex.c`, ends a line at `\n` or `\r`, and takes the other byte
//! of the pair along when it comes next, so `\r\n` and `\n\r` end one line, and `\n\n` and `\r\r`
//! end two. `matches_a_model_of_puc_rios_line_counting` checks the line map against a model of
//! that function on every source of up to seven bytes made from those two bytes, a letter, and
//! 0xFF, a byte that never appears in valid UTF-8.

use lunaferra_syntax::{Error, LineMap, MAX_SOURCE_LEN, SourceId};

/// Returns the line and column that the line map of `source` gives each offset, from 0 to the
/// length of `source`.
fn positions(source: &[u8]) -> Vec<(u32, u32)> {
    let lines = LineMap::new(source).expect("each test's source is shorter than the limit");
    let len = u32::try_from(source.len()).expect("each test's source is shorter than the limit");
    (0..=len).map(|offset| lines.line_col(offset)).collect()
}

/// Returns the line and column of every offset of `source`, from 0 to its length, found by
/// reading `source` the way PUC-Rio's lexer does: one byte at a time, where a line break moves
/// to the next line and takes the other byte of a pair along when it comes next.
fn model(source: &[u8]) -> Vec<(u32, u32)> {
    let mut positions = Vec::new();
    let (mut line, mut column) = (1, 1);
    let mut rest = source;
    while let Some((&byte, after)) = rest.split_first() {
        positions.push((line, column));
        rest = after;
        if byte == b'\n' || byte == b'\r' {
            if let Some((&next, after)) = rest.split_first()
                && (next == b'\n' || next == b'\r')
                && next != byte
            {
                positions.push((line, column + 1));
                rest = after;
            }
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    positions.push((line, column));
    positions
}

/// Returns every source of up to `max_len` bytes, each byte taken from `alphabet`.
fn every_source(alphabet: &[u8], max_len: u32) -> Vec<Vec<u8>> {
    let mut sources = vec![Vec::new()];
    let mut shorter = vec![Vec::new()];
    for _ in 0..max_len {
        let longer: Vec<Vec<u8>> = shorter
            .iter()
            .flat_map(|source: &Vec<u8>| {
                alphabet.iter().map(move |&byte| {
                    let mut source = source.clone();
                    source.push(byte);
                    source
                })
            })
            .collect();
        sources.extend(longer.iter().cloned());
        shorter = longer;
    }
    sources
}

#[test]
fn gives_an_empty_source_line_1_column_1() {
    assert_eq!(positions(b""), [(1, 1)]);
}

#[test]
fn counts_columns_in_bytes_along_one_line() {
    assert_eq!(positions(b"abc"), [(1, 1), (1, 2), (1, 3), (1, 4)]);
}

#[test]
fn ends_a_line_at_a_line_feed() {
    assert_eq!(positions(b"a\nb"), [(1, 1), (1, 2), (2, 1), (2, 2)]);
}

#[test]
fn ends_a_line_at_a_carriage_return() {
    assert_eq!(positions(b"a\rb"), [(1, 1), (1, 2), (2, 1), (2, 2)]);
}

#[test]
fn ends_one_line_at_a_carriage_return_and_a_line_feed() {
    assert_eq!(
        positions(b"a\r\nb"),
        [(1, 1), (1, 2), (1, 3), (2, 1), (2, 2)]
    );
}

#[test]
fn ends_one_line_at_a_line_feed_and_a_carriage_return() {
    assert_eq!(
        positions(b"a\n\rb"),
        [(1, 1), (1, 2), (1, 3), (2, 1), (2, 2)]
    );
}

#[test]
fn ends_two_lines_at_two_line_feeds() {
    assert_eq!(
        positions(b"a\n\nb"),
        [(1, 1), (1, 2), (2, 1), (3, 1), (3, 2)]
    );
}

#[test]
fn ends_two_lines_at_two_carriage_returns() {
    assert_eq!(
        positions(b"a\r\rb"),
        [(1, 1), (1, 2), (2, 1), (3, 1), (3, 2)]
    );
}

// After a pair, the next byte starts a line break of its own, even when it could have paired
// with the byte before it.
#[test]
fn pairs_each_byte_of_a_line_break_only_once() {
    assert_eq!(
        positions(b"\r\n\r\n"),
        [(1, 1), (1, 2), (2, 1), (2, 2), (3, 1)]
    );
    assert_eq!(
        positions(b"\n\r\n\r"),
        [(1, 1), (1, 2), (2, 1), (2, 2), (3, 1)]
    );
    assert_eq!(positions(b"\n\r\n"), [(1, 1), (1, 2), (2, 1), (3, 1)]);
    assert_eq!(positions(b"\r\n\r"), [(1, 1), (1, 2), (2, 1), (3, 1)]);
}

#[test]
fn puts_the_end_after_a_final_line_break_on_an_empty_line() {
    assert_eq!(positions(b"a\n"), [(1, 1), (1, 2), (2, 1)]);
    assert_eq!(positions(b"a\r\n"), [(1, 1), (1, 2), (1, 3), (2, 1)]);
}

#[test]
fn counts_each_byte_of_a_utf8_character_as_a_column() {
    // `é` is the two bytes C3 A9.
    assert_eq!(
        positions("é=1".as_bytes()),
        [(1, 1), (1, 2), (1, 3), (1, 4), (1, 5)]
    );
}

#[test]
fn reads_a_source_that_is_not_valid_utf8() {
    // FF never appears in UTF-8, C3 starts a two-byte character that the line feed cuts short,
    // and 80 continues a character that never started.
    assert_eq!(
        positions(b"\xff\xc3\n\x80x"),
        [(1, 1), (1, 2), (1, 3), (2, 1), (2, 2), (2, 3)]
    );
}

#[test]
fn gives_an_offset_past_the_end_the_position_of_the_end() {
    let lines = LineMap::new(b"a\nbc").unwrap();
    assert_eq!(lines.line_col(4), (2, 3));
    assert_eq!(lines.line_col(5), (2, 3));
    assert_eq!(lines.line_col(u32::MAX), (2, 3));
}

#[test]
fn matches_a_model_of_puc_rios_line_counting() {
    let sources = every_source(b"a\n\r\xff", 7);
    // 4^0 + 4^1 + ... + 4^7 sources.
    assert_eq!(sources.len(), 21_845);
    for source in &sources {
        assert_eq!(positions(source), model(source), "source {source:?}");
    }
}

#[test]
fn has_a_limit_one_byte_short_of_u32_max() {
    assert_eq!(MAX_SOURCE_LEN, 4_294_967_294);
}

// `vec![0; len]` asks the allocator for zeroed memory, which Linux maps lazily, so this test
// reserves 4 GB of address space but touches little memory. A 32-bit target can't hold a
// source that long.
#[cfg(target_pointer_width = "64")]
#[test]
fn rejects_a_source_one_byte_longer_than_the_limit() {
    let len = usize::try_from(u32::MAX).unwrap();
    let source = vec![0_u8; len];
    assert_eq!(
        LineMap::new(&source),
        Err(Error::SourceTooLong {
            len,
            limit: MAX_SOURCE_LEN
        })
    );
}

#[test]
fn describes_a_source_that_is_too_long() {
    let error = Error::SourceTooLong {
        len: 4_294_967_295,
        limit: MAX_SOURCE_LEN,
    };
    assert_eq!(
        error.to_string(),
        "source of 4294967295 bytes is longer than the limit of 4294967294 bytes"
    );
}

#[test]
fn keeps_the_number_of_a_source_id() {
    assert_eq!(SourceId::new(0).index(), 0);
    assert_eq!(SourceId::new(7).index(), 7);
    assert_eq!(SourceId::new(u32::MAX).index(), u32::MAX);
}
