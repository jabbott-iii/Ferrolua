//! Numerals: how far one reaches, as PUC-Rio Lua 5.5.1's `read_numeral` reads it, and its value,
//! as `luaO_str2num` converts it.
//!
//! A module of its own because a hexadecimal float needs a conversion of its own: the standard
//! library reads only decimal floats.

/// The value of a numeral: an integer or a float, as Lua 5.5 keeps the two apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    /// An integer.
    Integer(i64),
    /// A float.
    Float(f64),
}

/// Returns the offset just past the numeral whose first digit is at `digit`, as PUC-Rio's
/// `read_numeral` reads it.
///
/// The numeral takes every hexadecimal digit and `.` that follows, and an exponent mark (`e` or
/// `E`, or `p` or `P` after `0x`) with the sign after it, so `3-4` is three tokens but `1e-4` is
/// one. Then it takes one letter or `_` that touches it, so `3x` is one malformed numeral. The
/// value comes later, from [`numeral_value`], which rejects what isn't a valid number.
pub(super) fn numeral_end(source: &[u8], digit: usize) -> usize {
    let mut end = digit.saturating_add(1);
    let mut exponent = *b"eE";
    if source.get(digit) == Some(&b'0') && matches!(source.get(end), Some(b'x' | b'X')) {
        end = end.saturating_add(1);
        exponent = *b"pP";
    }
    while let Some(&byte) = source.get(end) {
        if exponent.contains(&byte) {
            end = end.saturating_add(1);
            if matches!(source.get(end), Some(b'+' | b'-')) {
                end = end.saturating_add(1);
            }
        } else if byte.is_ascii_hexdigit() || byte == b'.' {
            end = end.saturating_add(1);
        } else {
            break;
        }
    }
    match source.get(end) {
        Some(&byte) if byte.is_ascii_alphabetic() || byte == b'_' => end.saturating_add(1),
        _ => end,
    }
}

/// Returns the value of a numeral, the text of a [`TokenKind::Integer`] or [`TokenKind::Float`]
/// token, or `None` if `text` isn't a numeral.
///
/// It follows PUC-Rio Lua 5.5.1's `luaO_str2num` on the text its lexer reads as a numeral, with
/// no spaces or sign. A numeral is an integer if it is one: decimal digits whose value fits in
/// an `i64`, or `0x` and hexadecimal digits, whose value wraps around modulo 2^64. Otherwise it
/// is a float if C's `strtod` reads all of it, which takes decimal floats, such as `1.5e3`, and
/// C99's hexadecimal floats, such as `0x1.8p3`. A float rounds to the nearest value, and a tie
/// to the value whose last bit is 0, as glibc's `strtod` rounds, so `1e309` is infinite.
///
/// # Examples
///
/// ```
/// use lunaferra_syntax::{Number, numeral_value};
///
/// assert_eq!(numeral_value(b"0xff"), Some(Number::Integer(255)));
/// assert_eq!(numeral_value(b"0xffffffffffffffff"), Some(Number::Integer(-1)));
/// assert_eq!(numeral_value(b"9223372036854775808"), Some(Number::Float(9.223372036854775808e18)));
/// assert_eq!(numeral_value(b"0x1p-2"), Some(Number::Float(0.25)));
/// assert_eq!(numeral_value(b"0x"), None);
/// ```
///
/// [`TokenKind::Integer`]: crate::TokenKind::Integer
/// [`TokenKind::Float`]: crate::TokenKind::Float
pub fn numeral_value(text: &[u8]) -> Option<Number> {
    if let Some(value) = integer_value(text) {
        return Some(Number::Integer(value));
    }
    float_value(text).map(Number::Float)
}

/// Returns the value of `text` as an integer, as PUC-Rio's `l_str2int` reads it.
fn integer_value(text: &[u8]) -> Option<i64> {
    match text {
        [b'0', b'x' | b'X', digits @ ..] => {
            if digits.is_empty() || !digits.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            let value = digits.iter().fold(0_u64, |value, &digit| {
                value.wrapping_mul(16).wrapping_add(u64::from(hex(digit)))
            });
            Some(i64::from_ne_bytes(value.to_ne_bytes()))
        }
        _ => {
            if text.is_empty() || !text.iter().all(u8::is_ascii_digit) {
                return None;
            }
            text.iter().try_fold(0_i64, |value, &digit| {
                value.checked_mul(10)?.checked_add(i64::from(digit - b'0'))
            })
        }
    }
}

/// Returns the value of the hexadecimal digit `digit`, which the caller has checked is one.
fn hex(digit: u8) -> u8 {
    super::escape::hex_value(digit).unwrap_or(0)
}

/// Returns the value of `text` as a float, if `strtod` reads all of it.
fn float_value(text: &[u8]) -> Option<f64> {
    match text {
        [b'0', b'x' | b'X', rest @ ..] => {
            let parts = split_float(rest, u8::is_ascii_hexdigit, *b"pP")?;
            Some(hex_float(parts))
        }
        _ => {
            split_float(text, u8::is_ascii_digit, *b"eE")?;
            // `split_float` has checked that `text` is ASCII in the form that both `strtod` and
            // the standard library read, and the standard library rounds to the nearest float
            // as glibc's `strtod` does, so both conversions succeed.
            std::str::from_utf8(text)
                .ok()
                .and_then(|text| text.parse().ok())
        }
    }
}

/// The parts of a float numeral: its digits before and after the `.`, and the exponent after
/// the exponent mark, which is 0 when there is none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FloatParts<'a> {
    whole: &'a [u8],
    fraction: &'a [u8],
    exponent: i64,
}

/// Splits `text` into the parts of a float, as `strtod` reads one: digits that `is_digit`
/// accepts, with an optional `.` among them and at least one digit, then optionally an exponent
/// mark from `marks`, an optional sign, and at least one decimal digit. Returns `None` unless
/// that is all of `text`.
///
/// The exponent saturates at ±10^15, far past the exponent of any float that isn't 0 or
/// infinite, so a numeral with a huge exponent still gets the value `strtod` gives it.
fn split_float<'a>(
    text: &'a [u8],
    is_digit: fn(&u8) -> bool,
    marks: [u8; 2],
) -> Option<FloatParts<'a>> {
    let mantissa_len = text
        .iter()
        .position(|byte| marks.contains(byte))
        .unwrap_or(text.len());
    let (mantissa, exponent) = text.split_at(mantissa_len);
    let dot = mantissa
        .iter()
        .position(|&byte| byte == b'.')
        .unwrap_or(mantissa.len());
    let (whole, fraction) = mantissa.split_at(dot);
    // `fraction` starts with the `.`, if there is one.
    let fraction = fraction.get(1..).unwrap_or(&[]);
    let all_digits = whole.iter().all(is_digit) && fraction.iter().all(is_digit);
    if !all_digits || (whole.is_empty() && fraction.is_empty()) {
        return None;
    }
    let exponent = match exponent {
        [] => 0,
        [_, rest @ ..] => exponent_value(rest)?,
    };
    Some(FloatParts {
        whole,
        fraction,
        exponent,
    })
}

/// Returns the value of an exponent: an optional sign and at least one decimal digit, saturated
/// at ±10^15.
fn exponent_value(text: &[u8]) -> Option<i64> {
    const LIMIT: i64 = 1_000_000_000_000_000;
    let (negative, digits) = match text {
        [b'-', digits @ ..] => (true, digits),
        [b'+', digits @ ..] => (false, digits),
        digits => (false, digits),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let magnitude = digits.iter().fold(0_i64, |value, &digit| {
        (value * 10 + i64::from(digit - b'0')).min(LIMIT)
    });
    Some(if negative { -magnitude } else { magnitude })
}

/// Returns the float nearest to a hexadecimal float, with ties going to the float whose last
/// bit is 0, as glibc's `strtod` rounds.
///
/// The value is the hexadecimal digits times 2 to the power of the exponent, less 4 for each
/// digit after the `.`. Only the first 16 significant digits fit in a `u64`; any digit after
/// them that isn't 0 counts only toward rounding, as a "sticky" bit below the last one kept.
fn hex_float(parts: FloatParts<'_>) -> f64 {
    let mut significand: u64 = 0;
    let mut kept = 0;
    let mut sticky = false;
    let mut exponent = parts.exponent;
    let digits = parts.whole.iter().map(|&digit| (digit, false));
    let fraction = parts.fraction.iter().map(|&digit| (digit, true));
    for (digit, in_fraction) in digits.chain(fraction) {
        let value = hex(digit);
        if kept < 16 {
            if significand != 0 || value != 0 {
                significand = (significand << 4) | u64::from(value);
                kept += 1;
            }
            if in_fraction {
                exponent = exponent.saturating_sub(4);
            }
        } else {
            sticky |= value != 0;
            if !in_fraction {
                exponent = exponent.saturating_add(4);
            }
        }
    }
    round_to_f64(significand, exponent, sticky)
}

/// Returns the float nearest to `significand` times 2^`exponent`, plus a little more when
/// `sticky` is set, rounding a tie to the float whose last bit is 0.
fn round_to_f64(significand: u64, exponent: i64, sticky: bool) -> f64 {
    // The exponents of the smallest subnormal float's bit and of the largest float's top bit.
    const MIN_BIT: i64 = -1074;
    const MAX_TOP: i64 = 1023;
    if significand == 0 {
        return 0.0;
    }
    let top = exponent.saturating_add(i64::from(63 - significand.leading_zeros()));
    if top > MAX_TOP {
        return f64::INFINITY;
    }
    // The exponent of the last bit the float keeps: 53 bits below the top bit and its own, or
    // the smallest subnormal's bit.
    let last = top.saturating_sub(52).max(MIN_BIT);
    let shift = last.saturating_sub(exponent);
    let (mut kept, mut last) = if shift <= 0 {
        // `significand` has at most 53 bits from its top bit to `last`, so this can't overflow.
        let left = u32::try_from(-shift).unwrap_or(0);
        (significand << left, last)
    } else {
        (round_right(significand, shift, sticky), last)
    };
    if kept == 1 << 53 {
        kept >>= 1;
        last += 1;
    }
    f64_from_parts(kept, last)
}

/// Shifts `significand` right by `shift` bits, which is at least 1, rounding to the nearest and
/// a tie to even. `sticky` says the value has bits below `significand`'s that aren't 0.
fn round_right(significand: u64, shift: i64, sticky: bool) -> u64 {
    let Ok(shift) = u32::try_from(shift) else {
        return 0;
    };
    if shift > 64 {
        // The whole value is below half of the last bit kept.
        return 0;
    }
    let kept = significand.checked_shr(shift).unwrap_or(0);
    let dropped = significand ^ kept.checked_shl(shift).unwrap_or(0);
    let half = 1_u64 << (shift - 1);
    let round_up = dropped > half || (dropped == half && (sticky || kept & 1 == 1));
    kept + u64::from(round_up)
}

/// Returns the float `kept` times 2^`last`, where `kept` has at most 53 bits, and has 53 unless
/// `last` is the smallest subnormal's bit.
fn f64_from_parts(kept: u64, last: i64) -> f64 {
    const FRACTION_BITS: u64 = (1 << 52) - 1;
    if kept < 1 << 52 {
        return f64::from_bits(kept);
    }
    // The exponent field holds the exponent of the top bit, 52 above `last`, plus 1023.
    match u64::try_from(last + 52 + 1023) {
        Ok(field) if field < 2047 => f64::from_bits((field << 52) | (kept & FRACTION_BITS)),
        _ => f64::INFINITY,
    }
}

#[cfg(test)]
mod tests {
    use super::{Number, numeral_end, numeral_value};
    use lunaferra_testkit::Config;

    fn value(text: &str) -> Option<Number> {
        numeral_value(text.as_bytes())
    }

    /// Returns the value of `text` written with `Debug`, which tells 0.0 from -0.0 and writes
    /// each float with enough digits to tell it from every other.
    fn written(text: &str) -> String {
        format!("{:?}", value(text))
    }

    /// Returns the float with the bits `bits`, written as [`written`] writes a value.
    fn float(bits: u64) -> String {
        format!("{:?}", Some(Number::Float(f64::from_bits(bits))))
    }

    #[test]
    fn reads_decimal_integers() {
        assert_eq!(value("0"), Some(Number::Integer(0)));
        assert_eq!(value("007"), Some(Number::Integer(7)));
        assert_eq!(value("1234567890"), Some(Number::Integer(1_234_567_890)));
        assert_eq!(
            value("9223372036854775807"),
            Some(Number::Integer(i64::MAX))
        );
    }

    #[test]
    fn reads_a_decimal_integer_too_large_for_an_i64_as_a_float() {
        assert_eq!(
            value("9223372036854775808"),
            Some(Number::Float(9_223_372_036_854_775_808.0))
        );
        assert_eq!(
            value("1".repeat(400).as_str()),
            value("1.111111111111111e399")
        );
    }

    #[test]
    fn reads_hexadecimal_integers_wrapping_around() {
        assert_eq!(value("0xff"), Some(Number::Integer(255)));
        assert_eq!(value("0XfF"), Some(Number::Integer(255)));
        assert_eq!(value("0x7fffffffffffffff"), Some(Number::Integer(i64::MAX)));
        assert_eq!(value("0xffffffffffffffff"), Some(Number::Integer(-1)));
        assert_eq!(value("0x10000000000000000"), Some(Number::Integer(0)));
        assert_eq!(value("0x10000000000000001"), Some(Number::Integer(1)));
    }

    #[test]
    fn reads_decimal_floats() {
        assert_eq!(value("1.5"), Some(Number::Float(1.5)));
        assert_eq!(value(".5"), Some(Number::Float(0.5)));
        assert_eq!(value("5."), Some(Number::Float(5.0)));
        assert_eq!(value("3e2"), Some(Number::Float(300.0)));
        assert_eq!(value("3E-2"), Some(Number::Float(0.03)));
        assert_eq!(value("1e+3"), Some(Number::Float(1000.0)));
        assert_eq!(value(".5e1"), Some(Number::Float(5.0)));
        assert_eq!(value("1e309"), Some(Number::Float(f64::INFINITY)));
        assert_eq!(value("1e-400"), Some(Number::Float(0.0)));
    }

    #[test]
    fn reads_hexadecimal_floats() {
        assert_eq!(value("0x1p4"), Some(Number::Float(16.0)));
        assert_eq!(value("0x.8"), Some(Number::Float(0.5)));
        assert_eq!(value("0xA.8p0"), Some(Number::Float(10.5)));
        assert_eq!(value("0x1P-2"), Some(Number::Float(0.25)));
        assert_eq!(value("0x1p+2"), Some(Number::Float(4.0)));
        assert_eq!(value("0x1."), Some(Number::Float(1.0)));
        assert_eq!(value("0x0.0"), Some(Number::Float(0.0)));
        assert_eq!(value("0x0p99999"), Some(Number::Float(0.0)));
        assert_eq!(value("0x.01"), Some(Number::Float(1.0 / 256.0)));
    }

    /// Each numeral with the bits of the float that PUC-Rio Lua 5.5.1, built with GCC 13.3.0 on
    /// x86-64 Linux, gave it on 2026-11-25: `string.format("%016x", string.unpack("<i8",
    /// string.pack("<d", x)))`.
    #[test]
    fn rounds_hexadecimal_floats_as_puc_rio_does() {
        let cases: [(&str, u64); 19] = [
            ("0x1.fffffffffffffp1023", 0x7fef_ffff_ffff_ffff),
            ("0x1.fffffffffffff8p1023", 0x7ff0_0000_0000_0000),
            ("0x1p1024", 0x7ff0_0000_0000_0000),
            ("0x1p99999999999999999999", 0x7ff0_0000_0000_0000),
            ("0x1p-1022", 0x0010_0000_0000_0000),
            ("0x1p-1074", 0x0000_0000_0000_0001),
            ("0x1p-1075", 0x0000_0000_0000_0000),
            ("0x1.8p-1075", 0x0000_0000_0000_0001),
            ("0x3p-1076", 0x0000_0000_0000_0001),
            ("0x1p-99999999999999999999", 0x0000_0000_0000_0000),
            ("0x1.00000000000008p0", 0x3ff0_0000_0000_0000),
            ("0x1.00000000000018p0", 0x3ff0_0000_0000_0002),
            ("0x1.000000000000080000000001p0", 0x3ff0_0000_0000_0001),
            ("0x1.0000000000000fffffffffffffp0", 0x3ff0_0000_0000_0001),
            ("0xFFFFFFFFFFFFFFFFp-1138", 0x0000_0000_0000_0001),
            ("0x0.0000000000000000000000001p0", 0x39b0_0000_0000_0000),
            ("0x1p-1200", 0x0000_0000_0000_0000),
            ("0x1.fffffffffffffcp-1023", 0x0010_0000_0000_0000),
            ("0x1.ffffffffffffffp-1023", 0x0010_0000_0000_0000),
        ];
        for (text, bits) in cases {
            assert_eq!(written(text), float(bits), "{text}");
        }
    }

    #[test]
    fn rounds_hexadecimal_floats_as_a_wider_model_does() {
        // A model that rounds a significand of 65 to 128 bits in a `u128` instead of tracking a
        // sticky bit, for normal floats. Half the cases are made ties, which random bits almost
        // never are.
        Config::default().with_cases(2_000).assert(
            |source| {
                let high = source.below(u64::MAX) + 1;
                let low = source.below(u64::MAX);
                let exponent = i64::try_from(source.below(200)).unwrap() - 100;
                let tie = source.below(2) == 1;
                (high, low, exponent, tie)
            },
            |&(high, low, exponent, tie)| {
                let mut significand = (u128::from(high) << 64) | u128::from(low);
                // `high` isn't 0, so the significand has more than 64 bits and loses at least 12.
                let shift = (128 - significand.leading_zeros()) - 53;
                if tie {
                    significand = (significand >> shift << shift) | (1 << (shift - 1));
                }
                let text = format!("0x{significand:032x}p{exponent}");
                written(&text)
                    == format!(
                        "{:?}",
                        Some(Number::Float(model(significand, shift, exponent)))
                    )
            },
        );
    }

    /// Rounds `significand` times 2^`exponent` to the nearest float, a tie to even, for a value
    /// well inside the range of normal floats, dropping its last `shift` bits, at least 1.
    fn model(significand: u128, shift: u32, exponent: i64) -> f64 {
        let mut kept = significand >> shift;
        let dropped = significand - (kept << shift);
        let half = 1_u128 << (shift - 1);
        if dropped > half || (dropped == half && kept & 1 == 1) {
            kept += 1;
        }
        let power = exponent + i64::from(shift);
        let scale = 2_f64.powi(i32::try_from(power).unwrap());
        // `kept` has at most 54 bits, so each half and their sum are exact in a float, and so is
        // the product with a power of two.
        let high = f64::from(u32::try_from(kept >> 32).unwrap());
        let low = f64::from(u32::try_from(kept & 0xFFFF_FFFF).unwrap());
        (high * 4_294_967_296.0 + low) * scale
    }

    #[test]
    fn reads_back_every_float_written_exactly_in_hexadecimal() {
        Config::default().with_cases(2_000).assert(
            |source| source.below(0x7FF0_0000_0000_0000),
            |&bits| {
                let float = f64::from_bits(bits);
                let fraction = bits & ((1 << 52) - 1);
                let field = i64::try_from(bits >> 52).unwrap();
                let text = if field == 0 {
                    format!("0x0.{fraction:013x}p-1022")
                } else {
                    format!("0x1.{fraction:013x}p{}", field - 1023)
                };
                written(&text) == format!("{:?}", Some(Number::Float(float)))
            },
        );
    }

    #[test]
    fn rejects_malformed_numerals() {
        for text in [
            "0x", "0X", "0x.", "0x.p1", "0xg", "3x", "3_", "1e", "1e+", "1ef", "1.2.3", "1..2",
            "0x1p", "0x1p+", "0x1p-x", "0x1.8.5", ".0x5", "1in", "1n", "0x1e+", "",
        ] {
            assert_eq!(value(text), None, "{text}");
        }
    }

    #[test]
    fn reads_as_far_as_read_numeral_does() {
        let cases: [(&str, usize); 13] = [
            ("123 ", 3),
            ("3-4", 1),
            ("1e-4+", 4),
            ("1E+4", 4),
            ("0xe+1", 3),
            ("0x1p-4", 6),
            ("0X1P+4", 6),
            ("1..2.", 5),
            ("3x9", 2),
            ("3_", 2),
            ("0xg", 3),
            ("123abc", 6),
            ("1", 1),
        ];
        for (text, end) in cases {
            assert_eq!(numeral_end(text.as_bytes(), 0), end, "{text}");
        }
    }
}
