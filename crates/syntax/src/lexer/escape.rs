//! Escape sequences in short strings and short f-strings, and the long brackets of long strings
//! and long comments, read the way PUC-Rio Lua 5.5.1's `llex.c` reads them.
//!
//! A module of its own because short strings and f-strings share the escapes, and long strings,
//! long f-strings, and long comments share the brackets.

use super::LexErrorKind;

/// An escape sequence that failed, and how far the lexer read into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EscapeError {
    /// What is wrong with the escape.
    pub(super) kind: LexErrorKind,
    /// The offset of the byte that failed, or the length of the source if it ended first. The
    /// error's line is this byte's line.
    pub(super) at: usize,
    /// The offset just past the bytes the escape consumed: the failing byte is one of them,
    /// unless the source ended first, as in PUC-Rio's `esccheck`.
    pub(super) end: usize,
}

/// Reads the escape sequence whose `\` is at offset `start` of `source`, appends the bytes it
/// stands for to `out`, and returns the offset just past it.
///
/// A `\` at the end of the source consumes only itself and appends nothing, so the string that
/// holds it is reported as unfinished. `\z` skips the spaces and line breaks after it.
pub(super) fn read_escape(
    source: &[u8],
    start: usize,
    out: &mut Vec<u8>,
) -> Result<usize, EscapeError> {
    let letter = start.saturating_add(1);
    let after = letter.saturating_add(1);
    let byte = match source.get(letter) {
        None => return Ok(letter),
        Some(b'a') => 0x07,
        Some(b'b') => 0x08,
        Some(b'f') => 0x0C,
        Some(b'n') => b'\n',
        Some(b'r') => b'\r',
        Some(b't') => b'\t',
        Some(b'v') => 0x0B,
        Some(&quoted @ (b'\\' | b'"' | b'\'')) => quoted,
        Some(b'\n' | b'\r') => {
            out.push(b'\n');
            return Ok(line_break_end(source, letter));
        }
        Some(b'x') => {
            let (value, end) = hex_escape(source, after)?;
            out.push(value);
            return Ok(end);
        }
        Some(b'u') => {
            let (value, end) = utf8_escape(source, after)?;
            push_utf8(out, value);
            return Ok(end);
        }
        Some(b'z') => {
            let skipped = source.get(after..).unwrap_or(&[]);
            let spaces = skipped.iter().take_while(|&&byte| is_space(byte)).count();
            return Ok(after.saturating_add(spaces));
        }
        Some(b'0'..=b'9') => {
            let (value, end) = decimal_escape(source, letter)?;
            out.push(value);
            return Ok(end);
        }
        Some(_) => return Err(failure(source, LexErrorKind::InvalidEscape, letter)),
    };
    out.push(byte);
    Ok(after)
}

/// Returns the offset just past the line break at `at`: two bytes for `\r\n` or `\n\r`, as
/// PUC-Rio's `inclinenumber` pairs them, and one otherwise.
pub(super) fn line_break_end(source: &[u8], at: usize) -> usize {
    let next = at.saturating_add(1);
    match (source.get(at), source.get(next)) {
        (Some(b'\n'), Some(b'\r')) | (Some(b'\r'), Some(b'\n')) => next.saturating_add(1),
        _ => next,
    }
}

/// Returns whether `byte` is a space for `\z` and for PUC-Rio's `lisspace`: a space, `\t`, `\n`,
/// `\v`, `\f`, or `\r`.
const fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

/// Returns the error `kind` for the byte at `at`, which the escape consumes unless the source
/// ended first.
fn failure(source: &[u8], kind: LexErrorKind, at: usize) -> EscapeError {
    let end = if at < source.len() {
        at.saturating_add(1)
    } else {
        at
    };
    EscapeError { kind, at, end }
}

/// Returns the value of the hexadecimal digit at `at`, or the error that `\x` and `\u{` give for
/// a byte that isn't one.
fn hex_digit(source: &[u8], at: usize) -> Result<u8, EscapeError> {
    source
        .get(at)
        .and_then(|&byte| hex_value(byte))
        .ok_or_else(|| failure(source, LexErrorKind::HexDigitExpected, at))
}

/// Returns the value of the hexadecimal digit `byte`, in either case.
pub(super) const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Reads the two hexadecimal digits of `\xXX` from `at`, and returns their value and the offset
/// just past them.
fn hex_escape(source: &[u8], at: usize) -> Result<(u8, usize), EscapeError> {
    let high = hex_digit(source, at)?;
    let second = at.saturating_add(1);
    let low = hex_digit(source, second)?;
    Ok(((high << 4) | low, second.saturating_add(1)))
}

/// Reads `{XXX}` of `\u{XXX}` from `at`, and returns the value and the offset just past the `}`.
///
/// The value may be up to `7FFFFFFF`, as in PUC-Rio's `readutf8esc`, which checks before each
/// digit after the first that the value so far is at most `7FFFFFF`, so that the next digit
/// can't take it past that.
fn utf8_escape(source: &[u8], at: usize) -> Result<(u32, usize), EscapeError> {
    if source.get(at) != Some(&b'{') {
        return Err(failure(source, LexErrorKind::MissingOpeningBrace, at));
    }
    let first = at.saturating_add(1);
    let mut value = u32::from(hex_digit(source, first)?);
    let mut next = first.saturating_add(1);
    while let Some(digit) = source.get(next).and_then(|&byte| hex_value(byte)) {
        if value > 0x7FF_FFFF {
            return Err(failure(source, LexErrorKind::Utf8ValueTooLarge, next));
        }
        value = (value << 4) | u32::from(digit);
        next = next.saturating_add(1);
    }
    if source.get(next) != Some(&b'}') {
        return Err(failure(source, LexErrorKind::MissingClosingBrace, next));
    }
    Ok((value, next.saturating_add(1)))
}

/// Appends `value` in the UTF-8 of PUC-Rio's `luaO_utf8esc`, which takes values up to `7FFFFFFF`
/// and writes up to six bytes, the form UTF-8 had before it stopped at `10FFFF`. Surrogates
/// such as `D800` are written like any other value.
fn push_utf8(out: &mut Vec<u8>, value: u32) {
    let [low, ..] = value.to_le_bytes();
    if value < 0x80 {
        out.push(low);
        return;
    }
    let mut continuation = Vec::with_capacity(5);
    let mut rest = value;
    // The largest value that still fits in the first byte, which loses one bit for each
    // continuation byte.
    let mut first_max = 0x3F;
    loop {
        let [low, ..] = rest.to_le_bytes();
        continuation.push(0x80 | (low & 0x3F));
        rest >>= 6;
        first_max >>= 1;
        if rest <= first_max {
            break;
        }
    }
    let [first, ..] = ((!first_max << 1) | rest).to_le_bytes();
    out.push(first);
    out.extend(continuation.iter().rev());
}

/// Reads the up to three digits of `\ddd` from `at`, and returns their value and the offset
/// just past them.
fn decimal_escape(source: &[u8], at: usize) -> Result<(u8, usize), EscapeError> {
    let digits = source
        .get(at..)
        .unwrap_or(&[])
        .iter()
        .take(3)
        .take_while(|byte| byte.is_ascii_digit());
    let mut value: u32 = 0;
    let mut end = at;
    for &digit in digits {
        value = value * 10 + u32::from(digit - b'0');
        end = end.saturating_add(1);
    }
    u8::try_from(value)
        .map(|value| (value, end))
        .map_err(|_| failure(source, LexErrorKind::DecimalEscapeTooLarge, end))
}

/// What a `[` at the start of a possible long bracket opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Bracket {
    /// A long bracket with this many `=` signs, such as 2 for `[==[`.
    Long(usize),
    /// A `[` alone: no `=` signs and no second `[`.
    Single,
    /// `[` and some `=` signs, ending just before this offset, without a second `[`.
    Unfinished(usize),
}

/// Reads the brackets at `at`, where `source[at]` is `[`, as PUC-Rio's `skip_sep` does.
pub(super) fn open_bracket(source: &[u8], at: usize) -> Bracket {
    let first = at.saturating_add(1);
    let rest = source.get(first..).unwrap_or(&[]);
    let level = rest.iter().take_while(|&&byte| byte == b'=').count();
    let after = first.saturating_add(level);
    match (source.get(after), level) {
        (Some(b'['), _) => Bracket::Long(level),
        (_, 0) => Bracket::Single,
        _ => Bracket::Unfinished(after),
    }
}

/// Returns the offset just past the long bracket that opens at `at` with `level` `=` signs:
/// past its second `[`, and past a line break right after it, which isn't part of the text.
pub(super) fn long_body_start(source: &[u8], at: usize, level: usize) -> usize {
    let body = at.saturating_add(level).saturating_add(2);
    match source.get(body) {
        Some(b'\n' | b'\r') => line_break_end(source, body),
        _ => body,
    }
}

/// Returns whether a closing long bracket with `level` `=` signs, such as `]==]`, starts at
/// `at`.
pub(super) fn closes(source: &[u8], at: usize, level: usize) -> bool {
    let rest = source.get(at..).unwrap_or(&[]);
    rest.first() == Some(&b']')
        && rest
            .iter()
            .skip(1)
            .take_while(|&&byte| byte == b'=')
            .count()
            >= level
        && rest.get(level.saturating_add(1)) == Some(&b']')
}

/// Returns the offset just past the closing long bracket with `level` `=` signs that ends the
/// long string or comment whose text starts at `from`, or `None` if the source ends first.
pub(super) fn find_close(source: &[u8], from: usize, level: usize) -> Option<usize> {
    let rest = source.get(from..).unwrap_or(&[]);
    rest.iter()
        .enumerate()
        .filter(|&(_, &byte)| byte == b']')
        .map(|(offset, _)| from.saturating_add(offset))
        .find(|&at| closes(source, at, level))
        .map(|at| at.saturating_add(level).saturating_add(2))
}

#[cfg(test)]
mod tests {
    use super::{Bracket, EscapeError, find_close, open_bracket, push_utf8, read_escape};
    use crate::LexErrorKind;

    /// Reads the escape at the start of `source`, and returns the bytes it stands for and the
    /// offset just past it.
    fn escape(source: &[u8]) -> Result<(Vec<u8>, usize), EscapeError> {
        let mut out = Vec::new();
        read_escape(source, 0, &mut out).map(|end| (out, end))
    }

    #[test]
    fn reads_each_single_letter_escape() {
        let cases: [(&[u8], u8); 10] = [
            (b"\\a", 0x07),
            (b"\\b", 0x08),
            (b"\\f", 0x0C),
            (b"\\n", b'\n'),
            (b"\\r", b'\r'),
            (b"\\t", b'\t'),
            (b"\\v", 0x0B),
            (b"\\\\", b'\\'),
            (b"\\\"", b'"'),
            (b"\\'", b'\''),
        ];
        for (source, byte) in cases {
            assert_eq!(escape(source), Ok((vec![byte], 2)), "{source:?}");
        }
    }

    #[test]
    fn reads_a_backslash_and_a_line_break_as_one_line_feed() {
        assert_eq!(escape(b"\\\nx"), Ok((vec![b'\n'], 2)));
        assert_eq!(escape(b"\\\rx"), Ok((vec![b'\n'], 2)));
        assert_eq!(escape(b"\\\r\nx"), Ok((vec![b'\n'], 3)));
        assert_eq!(escape(b"\\\n\rx"), Ok((vec![b'\n'], 3)));
        assert_eq!(escape(b"\\\n\nx"), Ok((vec![b'\n'], 2)));
    }

    #[test]
    fn reads_hexadecimal_escapes_in_either_case() {
        assert_eq!(escape(b"\\x41"), Ok((vec![0x41], 4)));
        assert_eq!(escape(b"\\xfF"), Ok((vec![0xFF], 4)));
        assert_eq!(escape(b"\\x00"), Ok((vec![0], 4)));
    }

    #[test]
    fn reads_decimal_escapes_of_one_to_three_digits() {
        assert_eq!(escape(b"\\0"), Ok((vec![0], 2)));
        assert_eq!(escape(b"\\65x"), Ok((vec![65], 3)));
        assert_eq!(escape(b"\\0659"), Ok((vec![65], 4)));
        assert_eq!(escape(b"\\255"), Ok((vec![255], 4)));
    }

    #[test]
    fn reads_utf8_escapes_up_to_six_bytes() {
        let cases: [(&[u8], &[u8]); 8] = [
            (b"\\u{41}", b"A"),
            (b"\\u{7F}", &[0x7F]),
            (b"\\u{80}", &[0xC2, 0x80]),
            (b"\\u{7FF}", &[0xDF, 0xBF]),
            (b"\\u{800}", &[0xE0, 0xA0, 0x80]),
            (b"\\u{D800}", &[0xED, 0xA0, 0x80]),
            (b"\\u{10FFFF}", &[0xF4, 0x8F, 0xBF, 0xBF]),
            (b"\\u{7FFFFFFF}", &[0xFD, 0xBF, 0xBF, 0xBF, 0xBF, 0xBF]),
        ];
        for (source, bytes) in cases {
            assert_eq!(
                escape(source),
                Ok((bytes.to_vec(), source.len())),
                "{source:?}"
            );
        }
    }

    #[test]
    fn writes_each_length_of_utf8_at_its_first_value() {
        let cases: [(u32, &[u8]); 6] = [
            (0, &[0]),
            (0x80, &[0xC2, 0x80]),
            (0x800, &[0xE0, 0xA0, 0x80]),
            (0x1_0000, &[0xF0, 0x90, 0x80, 0x80]),
            (0x20_0000, &[0xF8, 0x88, 0x80, 0x80, 0x80]),
            (0x400_0000, &[0xFC, 0x84, 0x80, 0x80, 0x80, 0x80]),
        ];
        for (value, bytes) in cases {
            let mut out = Vec::new();
            push_utf8(&mut out, value);
            assert_eq!(out, bytes, "{value:#x}");
        }
    }

    #[test]
    fn accepts_leading_zeros_in_a_utf8_escape() {
        assert_eq!(escape(b"\\u{0000000041}"), Ok((b"A".to_vec(), 14)));
    }

    #[test]
    fn skips_spaces_and_line_breaks_after_z() {
        assert_eq!(escape(b"\\z \t\n\r\x0B\x0Cx"), Ok((Vec::new(), 8)));
        assert_eq!(escape(b"\\zx"), Ok((Vec::new(), 2)));
        assert_eq!(escape(b"\\z"), Ok((Vec::new(), 2)));
    }

    #[test]
    fn consumes_only_a_backslash_at_the_end() {
        assert_eq!(escape(b"\\"), Ok((Vec::new(), 1)));
    }

    #[test]
    fn rejects_an_unknown_escape_letter() {
        let error = EscapeError {
            kind: LexErrorKind::InvalidEscape,
            at: 1,
            end: 2,
        };
        assert_eq!(escape(b"\\q"), Err(error));
    }

    #[test]
    fn rejects_a_hexadecimal_escape_without_two_digits() {
        let kind = LexErrorKind::HexDigitExpected;
        assert_eq!(
            escape(b"\\xg1"),
            Err(EscapeError {
                kind,
                at: 2,
                end: 3
            })
        );
        assert_eq!(
            escape(b"\\x1g"),
            Err(EscapeError {
                kind,
                at: 3,
                end: 4
            })
        );
        assert_eq!(
            escape(b"\\x1"),
            Err(EscapeError {
                kind,
                at: 3,
                end: 3
            })
        );
        assert_eq!(
            escape(b"\\x"),
            Err(EscapeError {
                kind,
                at: 2,
                end: 2
            })
        );
    }

    #[test]
    fn rejects_a_decimal_escape_above_255() {
        let kind = LexErrorKind::DecimalEscapeTooLarge;
        assert_eq!(
            escape(b"\\256x"),
            Err(EscapeError {
                kind,
                at: 4,
                end: 5
            })
        );
        assert_eq!(
            escape(b"\\999"),
            Err(EscapeError {
                kind,
                at: 4,
                end: 4
            })
        );
    }

    #[test]
    fn rejects_each_broken_utf8_escape() {
        let cases: [(&[u8], LexErrorKind, usize, usize); 6] = [
            (b"\\u41", LexErrorKind::MissingOpeningBrace, 2, 3),
            (b"\\u", LexErrorKind::MissingOpeningBrace, 2, 2),
            (b"\\u{}", LexErrorKind::HexDigitExpected, 3, 4),
            (b"\\u{41", LexErrorKind::MissingClosingBrace, 5, 5),
            (b"\\u{41x", LexErrorKind::MissingClosingBrace, 5, 6),
            (b"\\u{80000000}", LexErrorKind::Utf8ValueTooLarge, 10, 11),
        ];
        for (source, kind, at, end) in cases {
            assert_eq!(
                escape(source),
                Err(EscapeError { kind, at, end }),
                "{source:?}"
            );
        }
    }

    #[test]
    fn accepts_the_largest_utf8_value_and_rejects_one_more_digit() {
        assert!(escape(b"\\u{7FFFFFFF}").is_ok());
        let error = EscapeError {
            kind: LexErrorKind::Utf8ValueTooLarge,
            at: 11,
            end: 12,
        };
        assert_eq!(escape(b"\\u{100000000}"), Err(error));
    }

    #[test]
    fn reads_long_brackets_as_skip_sep_does() {
        assert_eq!(open_bracket(b"[[", 0), Bracket::Long(0));
        assert_eq!(open_bracket(b"[==[", 0), Bracket::Long(2));
        assert_eq!(open_bracket(b"[x", 0), Bracket::Single);
        assert_eq!(open_bracket(b"[", 0), Bracket::Single);
        assert_eq!(open_bracket(b"[==x", 0), Bracket::Unfinished(3));
        assert_eq!(open_bracket(b"[=", 0), Bracket::Unfinished(2));
    }

    #[test]
    fn finds_only_a_closing_bracket_of_the_same_level() {
        assert_eq!(find_close(b"a]]", 0, 0), Some(3));
        assert_eq!(find_close(b"a]=]]==]", 0, 2), Some(8));
        assert_eq!(find_close(b"]=]", 0, 0), None);
        assert_eq!(find_close(b"]==", 0, 2), None);
        assert_eq!(find_close(b"", 0, 0), None);
    }
}
