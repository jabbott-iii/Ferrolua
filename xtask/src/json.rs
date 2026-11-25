//! A reader for JSON, the format in which Cargo and `llvm-cov` describe what they built and what the
//! tests ran. It follows RFC 8259 strictly, and keeps only what the tasks read: a number becomes
//! [`Value::Integer`] when it is an integer that fits in a `u64`, and [`Value::Number`] otherwise.
//!
//! `ferrolua-json` (task M6.9) will be Ferrolua's JSON crate, but `xtask` depends on no other crate,
//! so it has this reader of its own.

use std::fmt;

/// The deepest nesting of arrays and objects that [`parse`] accepts. Cargo and `llvm-cov` nest
/// their output fewer than 10 levels deep; the limit keeps malformed input from overflowing the
/// stack.
pub(crate) const MAX_DEPTH: usize = 128;

/// A JSON value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    /// `null`.
    Null,
    /// `true`.
    True,
    /// `false`.
    False,
    /// A number written as an integer from 0 to `u64::MAX`, with no sign, fraction, or exponent.
    Integer(u64),
    /// Any other number. Its value isn't kept, because no task reads one.
    Number,
    /// A string, with its escapes decoded.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object's members, in the order the input gives them.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// Returns the value of the first member named `name`, if this is an object that has one.
    pub(crate) fn get(&self, name: &str) -> Option<&Value> {
        let Value::Object(members) = self else {
            return None;
        };
        members
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    /// Returns the text of a string.
    pub(crate) fn as_str(&self) -> Option<&str> {
        if let Value::String(text) = self {
            Some(text)
        } else {
            None
        }
    }

    /// Returns the items of an array.
    pub(crate) fn as_array(&self) -> Option<&[Value]> {
        if let Value::Array(items) = self {
            Some(items)
        } else {
            None
        }
    }

    /// Returns the value of an integer from 0 to `u64::MAX`.
    pub(crate) fn as_u64(&self) -> Option<u64> {
        if let Value::Integer(value) = self {
            Some(*value)
        } else {
            None
        }
    }
}

/// Why [`parse`] rejected its input, and the offset of the byte where it found the problem: the
/// length of the input when the problem is that the input ended too soon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Error {
    /// The offset of the byte, from 0.
    pub(crate) offset: usize,
    /// What is wrong there.
    pub(crate) problem: &'static str,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.problem, self.offset)
    }
}

/// Parses `input` as one JSON value, with optional whitespace around it.
///
/// # Errors
///
/// Returns an [`Error`] when `input` isn't valid JSON, when a string in it isn't valid UTF-8, or
/// when its arrays and objects nest deeper than [`MAX_DEPTH`].
pub(crate) fn parse(input: &[u8]) -> Result<Value, Error> {
    let mut parser = Parser { input, offset: 0 };
    let value = parser.value(0)?;
    parser.skip_whitespace();
    match parser.peek() {
        None => Ok(value),
        Some(_) => Err(parser.error("expected the end of the input")),
    }
}

/// A position in the input of [`parse`].
struct Parser<'a> {
    input: &'a [u8],
    /// The offset of the next byte to read. It never passes the end of `input`.
    offset: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.input.get(self.offset).copied()
    }

    fn error(&self, problem: &'static str) -> Error {
        Error {
            offset: self.offset,
            problem,
        }
    }

    /// Moves past the next byte if it is `byte`, and says whether it was.
    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.offset += 1;
        }
        found
    }

    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.offset += 1;
        }
    }

    /// Reads a value inside `depth` arrays and objects.
    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Value::String),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b't') => self.word(b"true", Value::True),
            Some(b'f') => self.word(b"false", Value::False),
            Some(b'n') => self.word(b"null", Value::Null),
            _ => Err(self.error("expected a value")),
        }
    }

    /// Reads `word`, which spells `value`.
    fn word(&mut self, word: &[u8], value: Value) -> Result<Value, Error> {
        let rest = self.input.get(self.offset..).unwrap_or_default();
        if rest.starts_with(word) {
            self.offset += word.len();
            Ok(value)
        } else {
            Err(self.error("expected a value"))
        }
    }

    /// Moves past the `[` or `{` that opens an array or object inside `depth` others, and returns
    /// the depth of its contents.
    fn open(&mut self, depth: usize) -> Result<usize, Error> {
        if depth == MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        self.offset += 1;
        Ok(depth + 1)
    }

    fn array(&mut self, depth: usize) -> Result<Value, Error> {
        let depth = self.open(depth)?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.eat(b']') {
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value(depth)?);
            self.skip_whitespace();
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            if !self.eat(b',') {
                return Err(self.error("expected `,` or `]`"));
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, Error> {
        let depth = self.open(depth)?;
        let mut members = Vec::new();
        self.skip_whitespace();
        if self.eat(b'}') {
            return Ok(Value::Object(members));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a member name"));
            }
            let name = self.string()?;
            self.skip_whitespace();
            if !self.eat(b':') {
                return Err(self.error("expected `:`"));
            }
            members.push((name, self.value(depth)?));
            self.skip_whitespace();
            if self.eat(b'}') {
                return Ok(Value::Object(members));
            }
            if !self.eat(b',') {
                return Err(self.error("expected `,` or `}`"));
            }
        }
    }

    /// Reads a string, from the `"` that opens it.
    fn string(&mut self) -> Result<String, Error> {
        let start = self.offset;
        self.offset += 1;
        let mut bytes = Vec::new();
        loop {
            match self.peek() {
                Some(b'"') => break,
                Some(b'\\') => {
                    self.offset += 1;
                    let decoded = self.escape()?;
                    bytes.extend_from_slice(decoded.encode_utf8(&mut [0; 4]).as_bytes());
                }
                Some(0x00..=0x1f) => return Err(self.error("control character in a string")),
                Some(byte) => {
                    bytes.push(byte);
                    self.offset += 1;
                }
                None => return Err(self.error("unterminated string")),
            }
        }
        self.offset += 1;
        String::from_utf8(bytes).map_err(|_| Error {
            offset: start,
            problem: "invalid UTF-8 in a string",
        })
    }

    /// Decodes the escape after a backslash.
    fn escape(&mut self) -> Result<char, Error> {
        let decoded = match self.peek() {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.offset += 1;
                return self.code_point();
            }
            _ => return Err(self.error("invalid escape")),
        };
        self.offset += 1;
        Ok(decoded)
    }

    /// Decodes the four hexadecimal digits after `\u`, and the second escape of a surrogate pair,
    /// which writes a character above U+FFFF as two halves.
    fn code_point(&mut self) -> Result<char, Error> {
        let start = self.offset;
        let first = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&first) {
            if !(self.eat(b'\\') && self.eat(b'u')) {
                return Err(self.error("unpaired surrogate"));
            }
            let second = self.hex4()?;
            if !(0xDC00..0xE000).contains(&second) {
                return Err(self.error("unpaired surrogate"));
            }
            0x1_0000 + ((first - 0xD800) << 10) + (second - 0xDC00)
        } else {
            first
        };
        // Only a second half on its own, from U+DC00 to U+DFFF, is left that isn't a character.
        char::from_u32(code).ok_or(Error {
            offset: start,
            problem: "unpaired surrogate",
        })
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(byte @ b'0'..=b'9') => byte - b'0',
                Some(byte @ b'a'..=b'f') => byte - b'a' + 10,
                Some(byte @ b'A'..=b'F') => byte - b'A' + 10,
                _ => return Err(self.error("invalid hexadecimal digit")),
            };
            value = value * 16 + u32::from(digit);
            self.offset += 1;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<Value, Error> {
        let negative = self.eat(b'-');
        // The value of the integer part, until it no longer fits in a `u64`.
        let mut integer = Some(0_u64);
        if !self.eat(b'0') {
            if !matches!(self.peek(), Some(b'1'..=b'9')) {
                return Err(self.error("expected a digit"));
            }
            while let Some(byte @ b'0'..=b'9') = self.peek() {
                integer =
                    integer.and_then(|n| n.checked_mul(10)?.checked_add(u64::from(byte - b'0')));
                self.offset += 1;
            }
        }
        let fraction = self.eat(b'.');
        if fraction {
            self.digits()?;
        }
        let exponent = matches!(self.peek(), Some(b'e' | b'E'));
        if exponent {
            self.offset += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            self.digits()?;
        }
        Ok(match integer {
            Some(value) if !(negative || fraction || exponent) => Value::Integer(value),
            _ => Value::Number,
        })
    }

    /// Reads one or more decimal digits.
    fn digits(&mut self) -> Result<(), Error> {
        let start = self.offset;
        while let Some(b'0'..=b'9') = self.peek() {
            self.offset += 1;
        }
        if self.offset == start {
            Err(self.error("expected a digit"))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, MAX_DEPTH, Value, parse};

    fn error(offset: usize, problem: &'static str) -> Error {
        Error { offset, problem }
    }

    fn string(text: &str) -> Value {
        Value::String(text.to_owned())
    }

    #[test]
    fn reads_the_literals() {
        assert_eq!(parse(b"null"), Ok(Value::Null));
        assert_eq!(parse(b"true"), Ok(Value::True));
        assert_eq!(parse(b"false"), Ok(Value::False));
    }

    #[test]
    fn rejects_a_misspelled_literal() {
        assert_eq!(parse(b"nul"), Err(error(0, "expected a value")));
        assert_eq!(parse(b"tru"), Err(error(0, "expected a value")));
        assert_eq!(parse(b"fals"), Err(error(0, "expected a value")));
    }

    #[test]
    fn skips_whitespace_around_a_value() {
        assert_eq!(parse(b" \t\r\n7 \t\r\n"), Ok(Value::Integer(7)));
    }

    #[test]
    fn rejects_empty_input() {
        assert_eq!(parse(b""), Err(error(0, "expected a value")));
        assert_eq!(parse(b"  "), Err(error(2, "expected a value")));
    }

    #[test]
    fn rejects_a_byte_that_starts_no_value() {
        assert_eq!(parse(b"+1"), Err(error(0, "expected a value")));
        assert_eq!(parse(b"\xff"), Err(error(0, "expected a value")));
    }

    #[test]
    fn rejects_bytes_after_the_value() {
        assert_eq!(
            parse(b"1 2"),
            Err(error(2, "expected the end of the input"))
        );
        assert_eq!(parse(b"01"), Err(error(1, "expected the end of the input")));
    }

    #[test]
    fn reads_integers_up_to_u64_max() {
        assert_eq!(parse(b"0"), Ok(Value::Integer(0)));
        assert_eq!(parse(b"1"), Ok(Value::Integer(1)));
        assert_eq!(parse(b"1234567890"), Ok(Value::Integer(1_234_567_890)));
        assert_eq!(parse(b"18446744073709551615"), Ok(Value::Integer(u64::MAX)));
    }

    #[test]
    fn keeps_no_value_for_an_integer_past_u64_max() {
        // The last digit overflows the addition, and a further digit the multiplication.
        assert_eq!(parse(b"18446744073709551616"), Ok(Value::Number));
        assert_eq!(parse(b"184467440737095516150"), Ok(Value::Number));
    }

    #[test]
    fn keeps_no_value_for_a_negative_fractional_or_exponent_number() {
        assert_eq!(parse(b"-1"), Ok(Value::Number));
        assert_eq!(parse(b"-0"), Ok(Value::Number));
        assert_eq!(parse(b"1.5"), Ok(Value::Number));
        assert_eq!(parse(b"0.25"), Ok(Value::Number));
        assert_eq!(parse(b"1e5"), Ok(Value::Number));
        assert_eq!(parse(b"1E5"), Ok(Value::Number));
        assert_eq!(parse(b"1e+5"), Ok(Value::Number));
        assert_eq!(parse(b"77.5e-10"), Ok(Value::Number));
    }

    #[test]
    fn rejects_a_number_without_its_digits() {
        assert_eq!(parse(b"-"), Err(error(1, "expected a digit")));
        assert_eq!(parse(b"-x"), Err(error(1, "expected a digit")));
        assert_eq!(parse(b"1."), Err(error(2, "expected a digit")));
        assert_eq!(parse(b"1.e5"), Err(error(2, "expected a digit")));
        assert_eq!(parse(b"1e"), Err(error(2, "expected a digit")));
        assert_eq!(parse(b"1e+"), Err(error(3, "expected a digit")));
    }

    #[test]
    fn reads_a_string() {
        assert_eq!(parse(br#""""#), Ok(string("")));
        assert_eq!(parse(br#""src/lib.rs""#), Ok(string("src/lib.rs")));
    }

    #[test]
    fn reads_utf8_in_a_string() {
        assert_eq!(parse("\"é中😀\"".as_bytes()), Ok(string("é中😀")));
    }

    #[test]
    fn decodes_each_escape() {
        assert_eq!(
            parse(br#""\"\\\/\b\f\n\r\t""#),
            Ok(string("\"\\/\u{8}\u{c}\n\r\t"))
        );
    }

    #[test]
    fn decodes_unicode_escapes_in_each_case_of_hexadecimal() {
        assert_eq!(parse(br#""\u0041\u00e9\u00E9\u4e2D""#), Ok(string("Aéé中")));
    }

    #[test]
    fn decodes_a_surrogate_pair() {
        assert_eq!(parse(br#""\ud83d\ude00""#), Ok(string("😀")));
    }

    #[test]
    fn rejects_an_unpaired_surrogate() {
        // A first half with no escape after it, with another escape, or with a character that
        // isn't a second half; and a second half on its own.
        assert_eq!(parse(br#""\ud83d""#), Err(error(7, "unpaired surrogate")));
        assert_eq!(parse(br#""\ud83d\n""#), Err(error(8, "unpaired surrogate")));
        assert_eq!(
            parse(br#""\ud83d\u0041""#),
            Err(error(13, "unpaired surrogate"))
        );
        assert_eq!(parse(br#""\ude00""#), Err(error(3, "unpaired surrogate")));
    }

    #[test]
    fn rejects_an_invalid_escape() {
        assert_eq!(parse(br#""\x""#), Err(error(2, "invalid escape")));
        assert_eq!(parse(br#""\"#), Err(error(2, "invalid escape")));
    }

    #[test]
    fn rejects_a_unicode_escape_without_four_hexadecimal_digits() {
        assert_eq!(
            parse(br#""\u12g4""#),
            Err(error(5, "invalid hexadecimal digit"))
        );
        assert_eq!(
            parse(br#""\ud83d\u12""#),
            Err(error(11, "invalid hexadecimal digit"))
        );
    }

    #[test]
    fn rejects_an_unterminated_string() {
        assert_eq!(parse(br#""abc"#), Err(error(4, "unterminated string")));
    }

    #[test]
    fn rejects_a_control_character_in_a_string() {
        assert_eq!(
            parse(b"\"a\nb\""),
            Err(error(2, "control character in a string"))
        );
    }

    #[test]
    fn rejects_invalid_utf8_in_a_string() {
        assert_eq!(
            parse(b" \"a\xffb\""),
            Err(error(1, "invalid UTF-8 in a string"))
        );
    }

    #[test]
    fn reads_an_array() {
        assert_eq!(parse(b"[]"), Ok(Value::Array(Vec::new())));
        assert_eq!(parse(b"[ ]"), Ok(Value::Array(Vec::new())));
        assert_eq!(
            parse(b"[1, \"two\", [null]]"),
            Ok(Value::Array(vec![
                Value::Integer(1),
                string("two"),
                Value::Array(vec![Value::Null]),
            ]))
        );
    }

    #[test]
    fn rejects_a_malformed_array() {
        assert_eq!(parse(b"[1,]"), Err(error(3, "expected a value")));
        assert_eq!(parse(b"[1 2]"), Err(error(3, "expected `,` or `]`")));
        assert_eq!(parse(b"[1"), Err(error(2, "expected `,` or `]`")));
    }

    #[test]
    fn reads_an_object_with_its_members_in_order() {
        assert_eq!(parse(b"{}"), Ok(Value::Object(Vec::new())));
        assert_eq!(parse(b"{ }"), Ok(Value::Object(Vec::new())));
        assert_eq!(
            parse(br#"{"b": 1, "a" : {"c": true}}"#),
            Ok(Value::Object(vec![
                ("b".to_owned(), Value::Integer(1)),
                (
                    "a".to_owned(),
                    Value::Object(vec![("c".to_owned(), Value::True)])
                ),
            ]))
        );
    }

    #[test]
    fn rejects_a_malformed_object() {
        assert_eq!(parse(b"{1: 2}"), Err(error(1, "expected a member name")));
        assert_eq!(
            parse(br#"{"a": 1,}"#),
            Err(error(8, "expected a member name"))
        );
        assert_eq!(parse(br#"{"a\x": 1}"#), Err(error(4, "invalid escape")));
        assert_eq!(parse(br#"{"a" 1}"#), Err(error(5, "expected `:`")));
        assert_eq!(parse(br#"{"a": }"#), Err(error(6, "expected a value")));
        assert_eq!(
            parse(br#"{"a": 1 "b": 2}"#),
            Err(error(8, "expected `,` or `}`"))
        );
    }

    #[test]
    fn accepts_nesting_up_to_the_limit() {
        let mut input = "[".repeat(MAX_DEPTH);
        input.push_str(&"]".repeat(MAX_DEPTH));
        assert!(matches!(parse(input.as_bytes()), Ok(Value::Array(_))));
    }

    #[test]
    fn rejects_nesting_one_past_the_limit() {
        let mut arrays = "[".repeat(MAX_DEPTH + 1);
        arrays.push_str(&"]".repeat(MAX_DEPTH + 1));
        assert_eq!(
            parse(arrays.as_bytes()),
            Err(error(MAX_DEPTH, "nested too deeply"))
        );
        let mut objects = r#"{"a":"#.repeat(MAX_DEPTH);
        objects.push_str("{}");
        objects.push_str(&"}".repeat(MAX_DEPTH));
        assert_eq!(
            parse(objects.as_bytes()),
            Err(error(5 * MAX_DEPTH, "nested too deeply"))
        );
    }

    #[test]
    fn gets_the_first_member_with_a_name() {
        let object = parse(br#"{"a": 1, "b": 2, "a": 3}"#).unwrap();
        assert_eq!(object.get("a"), Some(&Value::Integer(1)));
        assert_eq!(object.get("b"), Some(&Value::Integer(2)));
        assert_eq!(object.get("c"), None);
        assert_eq!(Value::Array(Vec::new()).get("a"), None);
    }

    #[test]
    fn converts_only_the_matching_kind_of_value() {
        assert_eq!(string("a").as_str(), Some("a"));
        assert_eq!(Value::Null.as_str(), None);
        assert_eq!(
            Value::Array(vec![Value::Null]).as_array(),
            Some(&[Value::Null][..])
        );
        assert_eq!(Value::Null.as_array(), None);
        assert_eq!(Value::Integer(5).as_u64(), Some(5));
        assert_eq!(Value::Number.as_u64(), None);
    }

    #[test]
    fn describes_an_error_with_its_offset() {
        assert_eq!(
            error(12, "expected a value").to_string(),
            "expected a value at byte 12"
        );
    }
}
