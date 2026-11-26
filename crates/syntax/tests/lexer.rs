//! Tests of the lexer through the public API: every token form, the trivia around tokens, the
//! skipped start of a file, f-strings, each lexical error with PUC-Rio Lua 5.5.1's message, and
//! the round trip from tokens and trivia back to the source on generated input.

use lunaferra_syntax::{
    Error, LexError, LexErrorKind, Lexed, Limits, MAX_SOURCE_LEN, Near, Number, Origin, SourceId,
    Span, TokenKind, TriviaKind, lex, numeral_value,
};
use lunaferra_testkit::Config;

use TokenKind as T;

/// Lexes `source` as a string, as `load` reads one.
fn lexed(source: &[u8]) -> Lexed {
    lex(source, SourceId::new(0), Origin::String, &Limits::default())
        .expect("each test's source is short")
}

/// Lexes `source` as a file.
fn lexed_file(source: &[u8]) -> Lexed {
    lex(source, SourceId::new(0), Origin::File, &Limits::default())
        .expect("each test's source is short")
}

/// Returns the bytes of `span` in `source`.
fn text(source: &[u8], span: Span) -> &[u8] {
    let start = usize::try_from(span.start).expect("offsets fit in a usize");
    let end = usize::try_from(span.end).expect("offsets fit in a usize");
    source
        .get(start..end)
        .expect("each span is inside its source")
}

/// Returns each token of `source`, lexed as a string, with its text.
fn tokens(source: &str) -> Vec<(TokenKind, String)> {
    let bytes = source.as_bytes();
    lexed(bytes)
        .tokens()
        .iter()
        .map(|token| {
            let text = String::from_utf8_lossy(text(bytes, token.span)).into_owned();
            (token.kind, text)
        })
        .collect()
}

/// Returns the kind of each token of `source`, lexed as a string.
fn kinds(source: &[u8]) -> Vec<TokenKind> {
    lexed(source)
        .tokens()
        .iter()
        .map(|token| token.kind)
        .collect()
}

/// Returns `error` as `load(source, "=input")` reports it: `input:<line>: <message>`.
fn report(error: &LexError) -> Vec<u8> {
    let mut report = format!("input:{}: ", error.line).into_bytes();
    report.extend(error.message());
    report
}

/// Returns the report of the first error in `source`, lexed as a string.
fn first_error(source: &[u8]) -> Vec<u8> {
    let lexed = lexed(source);
    let error = lexed
        .errors()
        .first()
        .expect("each test's source has an error");
    report(error)
}

/// Writes out every token's leading trivia, bytes, and trailing trivia, in order.
fn round_trip(lexed: &Lexed, source: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for token in lexed.tokens() {
        for trivia in lexed.leading(token) {
            out.extend_from_slice(text(source, trivia.span));
        }
        out.extend_from_slice(text(source, token.span));
        for trivia in lexed.trailing(token) {
            out.extend_from_slice(text(source, trivia.span));
        }
    }
    out
}

fn owned(pairs: &[(TokenKind, &str)]) -> Vec<(TokenKind, String)> {
    pairs
        .iter()
        .map(|&(kind, text)| (kind, text.to_owned()))
        .collect()
}

#[test]
fn lexes_the_24_reserved_words() {
    let source = "and break do else elseif end false for function global goto if in is local \
                  nil not or repeat return then true until while";
    use TokenKind::*;
    let expected = [
        And, Break, Do, Else, Elseif, End, False, For, Function, Global, Goto, If, In, Is, Local,
        Nil, Not, Or, Repeat, Return, Then, True, Until, While, Eof,
    ];
    assert_eq!(kinds(source.as_bytes()), expected);
}

#[test]
fn lexes_contextual_keywords_and_other_words_as_names() {
    let source = b"nonlocal class extends type super _ _x1 f F ends iff And";
    assert_eq!(kinds(source), [vec![T::Name; 12], vec![T::Eof]].concat());
}

#[test]
fn ends_a_name_at_a_byte_outside_ascii() {
    assert_eq!(
        tokens("x\u{e9}"),
        owned(&[
            (T::Name, "x"),
            (T::Unknown, "\u{FFFD}"),
            (T::Unknown, "\u{FFFD}"),
            (T::Eof, "")
        ])
    );
}

#[test]
fn lexes_every_operator_and_symbol() {
    let cases: [(&str, TokenKind); 54] = [
        ("+", T::Plus),
        ("-", T::Minus),
        ("*", T::Star),
        ("/", T::Slash),
        ("//", T::SlashSlash),
        ("%", T::Percent),
        ("**", T::StarStar),
        ("^", T::Caret),
        ("#", T::Hash),
        ("&", T::Ampersand),
        ("~", T::Tilde),
        ("|", T::Pipe),
        ("<<", T::LessLess),
        (">>", T::GreaterGreater),
        ("==", T::EqualEqual),
        ("!=", T::BangEqual),
        ("<=", T::LessEqual),
        (">=", T::GreaterEqual),
        ("<", T::Less),
        (">", T::Greater),
        ("=", T::Equal),
        ("+=", T::PlusEqual),
        ("-=", T::MinusEqual),
        ("*=", T::StarEqual),
        ("/=", T::SlashEqual),
        ("//=", T::SlashSlashEqual),
        ("%=", T::PercentEqual),
        ("**=", T::StarStarEqual),
        ("&=", T::AmpersandEqual),
        ("|=", T::PipeEqual),
        ("^=", T::CaretEqual),
        ("<<=", T::LessLessEqual),
        (">>=", T::GreaterGreaterEqual),
        ("(", T::LeftParen),
        (")", T::RightParen),
        ("{", T::LeftBrace),
        ("}", T::RightBrace),
        ("[", T::LeftBracket),
        ("]", T::RightBracket),
        ("::", T::ColonColon),
        (";", T::Semicolon),
        (":", T::Colon),
        (",", T::Comma),
        (".", T::Dot),
        ("...", T::DotDotDot),
        ("?", T::Question),
        ("->", T::Arrow),
        ("..", T::DotDot),
        ("~=", T::TildeEqual),
        ("!", T::Unknown),
        ("@", T::Unknown),
        ("$", T::Unknown),
        ("`", T::Unknown),
        ("\\", T::Unknown),
    ];
    for (spelling, kind) in cases {
        assert_eq!(
            tokens(spelling),
            owned(&[(kind, spelling), (T::Eof, "")]),
            "{spelling}"
        );
    }
}

#[test]
fn takes_the_longest_operator() {
    assert_eq!(
        kinds(b"a**=b**c*=d*e"),
        [
            T::Name,
            T::StarStarEqual,
            T::Name,
            T::StarStar,
            T::Name,
            T::StarEqual,
            T::Name,
            T::Star,
            T::Name,
            T::Eof
        ]
    );
    assert_eq!(
        kinds(b"<<=<<<=< >>=>>>=>"),
        [
            T::LessLessEqual,
            T::LessLess,
            T::LessEqual,
            T::Less,
            T::GreaterGreaterEqual,
            T::GreaterGreater,
            T::GreaterEqual,
            T::Greater,
            T::Eof
        ]
    );
    assert_eq!(
        kinds(b"//=///=/"),
        [
            T::SlashSlashEqual,
            T::SlashSlash,
            T::SlashEqual,
            T::Slash,
            T::Eof
        ]
    );
    assert_eq!(
        kinds(b"===!==!"),
        [
            T::EqualEqual,
            T::Equal,
            T::BangEqual,
            T::Equal,
            T::Unknown,
            T::Eof
        ]
    );
    assert_eq!(kinds(b":::"), [T::ColonColon, T::Colon, T::Eof]);
}

#[test]
fn reads_minus_equals_minus_as_two_tokens() {
    assert_eq!(
        kinds(b"x -=- 1"),
        [T::Name, T::MinusEqual, T::Minus, T::Integer, T::Eof]
    );
    assert_eq!(kinds(b"a->b"), [T::Name, T::Arrow, T::Name, T::Eof]);
}

#[test]
fn starts_a_comment_at_two_minus_signs_before_any_operator() {
    let lexed = lexed(b"x--=1");
    let kinds: Vec<TokenKind> = lexed.tokens().iter().map(|token| token.kind).collect();
    assert_eq!(kinds, [T::Name, T::Eof]);
    let trailing = lexed.trailing(&lexed.tokens()[0]);
    assert_eq!(trailing.len(), 1);
    assert_eq!(trailing[0].kind, TriviaKind::Comment);
}

#[test]
fn reads_luas_removed_operators_for_the_parser_to_report() {
    assert_eq!(
        kinds(b"a .. b ~= c ~ d"),
        [
            T::Name,
            T::DotDot,
            T::Name,
            T::TildeEqual,
            T::Name,
            T::Tilde,
            T::Name,
            T::Eof
        ]
    );
}

#[test]
fn reads_dots_and_numerals_that_start_with_a_dot() {
    assert_eq!(
        tokens("... .. . .5 ..5 ...."),
        owned(&[
            (T::DotDotDot, "..."),
            (T::DotDot, ".."),
            (T::Dot, "."),
            (T::Float, ".5"),
            (T::DotDot, ".."),
            (T::Integer, "5"),
            (T::DotDotDot, "..."),
            (T::Dot, "."),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn lexes_integers_and_floats_in_decimal_and_hexadecimal() {
    let source = "0 007 9223372036854775807 9223372036854775808 0xff 0XFF 0xffffffffffffffff \
                  1.5 .5 5. 3e2 3E-2 1e+3 0x1p4 0x.8 0xA.8p0 0x1P-2 1e309";
    let kinds: Vec<TokenKind> = tokens(source).into_iter().map(|(kind, _)| kind).collect();
    let mut expected = vec![T::Integer, T::Integer, T::Integer, T::Float];
    expected.extend([T::Integer; 3]);
    expected.extend([T::Float; 11]);
    expected.push(T::Eof);
    assert_eq!(kinds, expected);
}

#[test]
fn gives_each_numeral_token_its_value() {
    let source = b"9223372036854775808 0xffffffffffffffff 0x1p-2 1e309";
    let lexed = lexed(source);
    let values: Vec<Option<Number>> = lexed
        .tokens()
        .iter()
        .map(|token| numeral_value(text(source, token.span)))
        .collect();
    assert_eq!(
        values,
        [
            Some(Number::Float(9_223_372_036_854_775_808.0)),
            Some(Number::Integer(-1)),
            Some(Number::Float(0.25)),
            Some(Number::Float(f64::INFINITY)),
            None,
        ]
    );
}

#[test]
fn splits_a_minus_sign_and_a_hexadecimal_e_from_a_numeral() {
    assert_eq!(
        tokens("3-4 0xe+1 1e-4"),
        owned(&[
            (T::Integer, "3"),
            (T::Minus, "-"),
            (T::Integer, "4"),
            (T::Integer, "0xe"),
            (T::Plus, "+"),
            (T::Integer, "1"),
            (T::Float, "1e-4"),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn lexes_short_strings_with_every_escape() {
    let source = r#""a\tb" 'it\'s' "q\"q" "\\" "\a\b\f\n\r\t\v" "\x41\u{48}\65\0" "a\z
        b" "a\
b" '"' "'""#;
    let kinds = kinds(source.as_bytes());
    assert_eq!(kinds, [vec![T::String; 10], vec![T::Eof]].concat());
}

#[test]
fn lexes_long_strings_of_every_level() {
    assert_eq!(
        tokens("[[a]] [==[b]]c]=]]==] [=[\nd]=] [[]]"),
        owned(&[
            (T::String, "[[a]]"),
            (T::String, "[==[b]]c]=]]==]"),
            (T::String, "[=[\nd]=]"),
            (T::String, "[[]]"),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn reads_a_bracket_without_a_second_bracket_or_equals_signs_as_a_bracket() {
    assert_eq!(
        kinds(b"t[1] [ [1] ]"),
        [
            T::Name,
            T::LeftBracket,
            T::Integer,
            T::RightBracket,
            T::LeftBracket,
            T::LeftBracket,
            T::Integer,
            T::RightBracket,
            T::RightBracket,
            T::Eof
        ]
    );
}

/// Each broken chunk with the message that `load(source, "=input")` returned for it on PUC-Rio
/// Lua 5.5.1, built with GCC 13.3.0 on x86-64 Linux, on 2026-11-25. A script wrote each source
/// as a Lua string of `\ddd` escapes, ran it, and wrote this table from the output.
const PUC_RIO_MESSAGES: [(&[u8], &[u8]); 54] = [
    (b"x = 0x", b"input:1: malformed number near '0x'"),
    (b"x = 3x", b"input:1: malformed number near '3x'"),
    (b"x = 1..2", b"input:1: malformed number near '1..2'"),
    (b"x = 0xg", b"input:1: malformed number near '0xg'"),
    (b"x = 1e", b"input:1: malformed number near '1e'"),
    (b"x = 1e+", b"input:1: malformed number near '1e+'"),
    (b"x = .0x5", b"input:1: malformed number near '.0x5'"),
    (b"x = 3_", b"input:1: malformed number near '3_'"),
    (b"x = 0x1p", b"input:1: malformed number near '0x1p'"),
    (b"x = 1.2.3", b"input:1: malformed number near '1.2.3'"),
    (b"x = 08x", b"input:1: malformed number near '08x'"),
    (b"x = 0x1p-x", b"input:1: malformed number near '0x1p-x'"),
    (b"x = 123abc", b"input:1: malformed number near '123abc'"),
    (b"x = \"abc\ny", b"input:1: unfinished string near '\"abc'"),
    (b"x = \"abc", b"input:1: unfinished string near <eof>"),
    (
        b"x = 'a\\qb'",
        b"input:1: invalid escape sequence near ''a\\q'",
    ),
    (
        b"x = \"\\65\\q\"",
        b"input:1: invalid escape sequence near '\"A\\q'",
    ),
    (
        b"x = \"\\xZZ\"",
        b"input:1: hexadecimal digit expected near '\"\\xZ'",
    ),
    (
        b"x = \"\\x5\"",
        b"input:1: hexadecimal digit expected near '\"\\x5\"'",
    ),
    (
        b"x = \"\\x\"",
        b"input:1: hexadecimal digit expected near '\"\\x\"'",
    ),
    (b"x = \"\\u{48\"", b"input:1: missing '}' near '\"\\u{48\"'"),
    (b"x = \"\\u48\"", b"input:1: missing '{' near '\"\\u4'"),
    (
        b"x = \"\\u{}\"",
        b"input:1: hexadecimal digit expected near '\"\\u{}'",
    ),
    (
        b"x = \"\\u{80000000}\"",
        b"input:1: UTF-8 value too large near '\"\\u{80000000'",
    ),
    (
        b"x = \"\\u{7FFFFFFF}\\q\"",
        b"input:1: invalid escape sequence near '\"\xFD\xBF\xBF\xBF\xBF\xBF\\q'",
    ),
    (
        b"x = \"\\256\"",
        b"input:1: decimal escape too large near '\"\\256\"'",
    ),
    (
        b"x = \"\\2560\"",
        b"input:1: decimal escape too large near '\"\\2560'",
    ),
    (
        b"x = \"\\999",
        b"input:1: decimal escape too large near '\"\\999'",
    ),
    (
        b"x = \"a\\z  \n  b\\q\"",
        b"input:2: invalid escape sequence near '\"ab\\q'",
    ),
    (
        b"x = \"a\\\nb\\q\"",
        b"input:2: invalid escape sequence near '\"a\nb\\q'",
    ),
    (b"x = \"\\", b"input:1: unfinished string near <eof>"),
    (b"x = \"a\\0b\n", b"input:1: unfinished string near '\"a'"),
    (b"x = \"\r", b"input:1: unfinished string near '\"'"),
    (
        b"x = \"a\\\r\nb\\q\"",
        b"input:2: invalid escape sequence near '\"a\nb\\q'",
    ),
    (
        b"x = \"\\xff\n",
        b"input:1: unfinished string near '\"\xFF'",
    ),
    (
        b"x = \"\\u{D800}\n",
        b"input:1: unfinished string near '\"\xED\xA0\x80'",
    ),
    (
        b"x = \"a\\tb\n",
        b"input:1: unfinished string near '\"a\tb'",
    ),
    (
        b"x = \"\\x4",
        b"input:1: hexadecimal digit expected near '\"\\x4'",
    ),
    (b"x = \"\\u{4", b"input:1: missing '}' near '\"\\u{4'"),
    (b"x = \"\\u", b"input:1: missing '{' near '\"\\u'"),
    (
        b"x = [[abc",
        b"input:1: unfinished long string (starting at line 1) near <eof>",
    ),
    (
        b"x = [==[a]]",
        b"input:1: unfinished long string (starting at line 1) near <eof>",
    ),
    (
        b"\n\nx = [[",
        b"input:3: unfinished long string (starting at line 3) near <eof>",
    ),
    (
        b"--[[ unfinished",
        b"input:1: unfinished long comment (starting at line 1) near <eof>",
    ),
    (
        b"\n--[==[ x ]]",
        b"input:2: unfinished long comment (starting at line 2) near <eof>",
    ),
    (
        b"x = [=x",
        b"input:1: invalid long string delimiter near '[='",
    ),
    (
        b"x = [==",
        b"input:1: invalid long string delimiter near '[=='",
    ),
    (b"--[=x\nx = 0x", b"input:2: malformed number near '0x'"),
    (b"\r\n\r\nx = 0x", b"input:3: malformed number near '0x'"),
    (b"\n\rx = 0x", b"input:2: malformed number near '0x'"),
    (b"\r\rx = 0x", b"input:3: malformed number near '0x'"),
    (b"x = [[\n]] .. 0x", b"input:2: malformed number near '0x'"),
    (
        b"x = \"\\z\n\n\n\\q\"",
        b"input:4: invalid escape sequence near '\"\\q'",
    ),
    (
        b"x = \"a\xFFb\\q\"",
        b"input:1: invalid escape sequence near '\"a\xFFb\\q'",
    ),
];

#[test]
fn gives_puc_rios_message_for_each_lexical_error() {
    for (source, message) in PUC_RIO_MESSAGES {
        assert_eq!(
            String::from_utf8_lossy(&first_error(source)),
            String::from_utf8_lossy(message),
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

#[test]
fn gives_each_lexical_error_its_kind_and_span() {
    let source = b"x = 0x y";
    let lexed = lexed(source);
    let error = LexError {
        kind: LexErrorKind::MalformedNumber,
        span: Span {
            source: SourceId::new(0),
            start: 4,
            end: 6,
        },
        line: 1,
        near: Near::Text(b"0x".to_vec()),
    };
    assert_eq!(lexed.errors(), [error]);
    assert_eq!(
        kinds(source),
        [T::Name, T::Equal, T::Error, T::Name, T::Eof]
    );
}

#[test]
fn names_the_line_where_a_long_string_starts_and_where_the_source_ends() {
    let lexed = lexed(b"x = [[\n\n");
    let error = &lexed.errors()[0];
    assert_eq!(
        error.kind,
        LexErrorKind::UnfinishedLongString { start_line: 1 }
    );
    assert_eq!(error.line, 3);
    assert_eq!(error.near, Near::Eof);
}

#[test]
fn goes_on_after_an_error_and_reports_one_error_per_token() {
    let source = b"a = \"\\q\\q\" b = 0x c = \"\\x\" d = 'e\nf";
    let lexed = lexed(source);
    let kinds: Vec<TokenKind> = lexed.tokens().iter().map(|token| token.kind).collect();
    assert_eq!(
        kinds,
        [
            T::Name,
            T::Equal,
            T::Error,
            T::Name,
            T::Equal,
            T::Error,
            T::Name,
            T::Equal,
            T::Error,
            T::Name,
            T::Equal,
            T::Error,
            T::Name,
            T::Eof
        ]
    );
    let messages: Vec<String> = lexed.errors().iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        [
            "invalid escape sequence near '\"\\q'",
            "malformed number near '0x'",
            "hexadecimal digit expected near '\"\\x\"'",
            "unfinished string near ''e'",
        ]
    );
}

#[test]
fn ends_a_string_with_an_error_at_its_closing_quote() {
    assert_eq!(
        tokens(r#""\q" x"#),
        owned(&[(T::Error, r#""\q""#), (T::Name, "x"), (T::Eof, "")])
    );
}

#[test]
fn writes_bytes_outside_utf8_in_a_message_as_replacement_characters() {
    let lexed = lexed(b"x = \"\xFF\n");
    let error = &lexed.errors()[0];
    assert_eq!(error.message(), b"unfinished string near '\"\xFF'");
    assert_eq!(error.to_string(), "unfinished string near '\"\u{FFFD}'");
}

#[test]
fn describes_each_kind_of_lexical_error() {
    let cases = [
        (LexErrorKind::MalformedNumber, "malformed number"),
        (LexErrorKind::UnfinishedString, "unfinished string"),
        (
            LexErrorKind::UnfinishedLongString { start_line: 7 },
            "unfinished long string (starting at line 7)",
        ),
        (
            LexErrorKind::UnfinishedLongComment { start_line: 8 },
            "unfinished long comment (starting at line 8)",
        ),
        (
            LexErrorKind::InvalidLongStringDelimiter,
            "invalid long string delimiter",
        ),
        (LexErrorKind::InvalidEscape, "invalid escape sequence"),
        (LexErrorKind::HexDigitExpected, "hexadecimal digit expected"),
        (LexErrorKind::MissingOpeningBrace, "missing '{'"),
        (LexErrorKind::MissingClosingBrace, "missing '}'"),
        (LexErrorKind::Utf8ValueTooLarge, "UTF-8 value too large"),
        (
            LexErrorKind::DecimalEscapeTooLarge,
            "decimal escape too large",
        ),
        (LexErrorKind::SingleClosingBrace, "single '}' in f-string"),
    ];
    for (kind, words) in cases {
        assert_eq!(kind.to_string(), words);
    }
}

#[test]
fn keeps_spaces_line_breaks_and_comments_as_trivia() {
    let source = b"  x \t-- one\r\n\x0B\x0C--[==[ two\n]==] y --[[ three ]] \n\n";
    let lexed = lexed(source);
    let kinds: Vec<TriviaKind> = lexed.trivia().iter().map(|trivia| trivia.kind).collect();
    use TriviaKind::*;
    assert_eq!(
        kinds,
        [
            Whitespace,
            Whitespace,
            Comment,
            LineBreak,
            Whitespace,
            LongComment,
            Whitespace,
            Whitespace,
            LongComment,
            Whitespace,
            LineBreak,
            LineBreak
        ]
    );
    let pieces: Vec<&[u8]> = lexed
        .trivia()
        .iter()
        .map(|trivia| text(source, trivia.span))
        .collect();
    assert_eq!(pieces[2], b"-- one");
    assert_eq!(pieces[3], b"\r\n");
    assert_eq!(pieces[5], b"--[==[ two\n]==]");
}

#[test]
fn gives_a_token_the_trivia_on_its_line_as_trailing_trivia() {
    let source = b"-- lead\nx -- after\n  y --[[ a\nb ]] z";
    let lexed = lexed(source);
    let tokens = lexed.tokens();
    let pieces = |trivia: &[lunaferra_syntax::Trivia]| -> Vec<Vec<u8>> {
        trivia
            .iter()
            .map(|trivia| text(source, trivia.span).to_vec())
            .collect()
    };
    assert_eq!(
        pieces(lexed.leading(&tokens[0])),
        [b"-- lead".to_vec(), b"\n".to_vec()]
    );
    assert_eq!(
        pieces(lexed.trailing(&tokens[0])),
        [b" ".to_vec(), b"-- after".to_vec()]
    );
    assert_eq!(
        pieces(lexed.leading(&tokens[1])),
        [b"\n".to_vec(), b"  ".to_vec()]
    );
    assert_eq!(
        pieces(lexed.trailing(&tokens[1])),
        [b" ".to_vec(), b"--[[ a\nb ]]".to_vec(), b" ".to_vec()]
    );
    assert!(lexed.leading(&tokens[2]).is_empty());
}

#[test]
fn reads_a_long_bracket_comment_without_a_second_bracket_as_a_short_comment() {
    let lexed = lexed(b"--[==x\ny");
    assert_eq!(lexed.trivia()[0].kind, TriviaKind::Comment);
    assert!(lexed.errors().is_empty());
}

#[test]
fn gives_back_a_program_from_its_tokens_and_trivia() {
    let source = b"#!x\n\xEF\xBB\xBF local t = { [1] = \"a\\z\n b\", f\"{x!r:>8} {{\" }\r\n\
                   -- done\n--[[ end ]]  ";
    for origin in [Origin::String, Origin::File] {
        let lexed = lex(source, SourceId::new(3), origin, &Limits::default()).unwrap();
        assert_eq!(round_trip(&lexed, source), source);
        let last = lexed.tokens().last().unwrap();
        let len = u32::try_from(source.len()).unwrap();
        assert_eq!(last.kind, T::Eof);
        assert_eq!(last.span.source, SourceId::new(3));
        assert_eq!((last.span.start, last.span.end), (len, len));
    }
}

#[test]
fn ends_with_an_empty_eof_token_for_an_empty_source() {
    let lexed = lexed(b"");
    assert_eq!(lexed.tokens().len(), 1);
    assert_eq!(lexed.tokens()[0].kind, T::Eof);
    assert!(lexed.trivia().is_empty());
    assert!(lexed.errors().is_empty());
}

#[test]
fn skips_a_files_byte_order_mark_and_first_line_starting_with_a_hash() {
    let source = b"\xEF\xBB\xBF#!/usr/bin/env lunaferra\nx";
    let lexed = lexed_file(source);
    let kinds: Vec<TriviaKind> = lexed.trivia().iter().map(|trivia| trivia.kind).collect();
    assert_eq!(
        kinds,
        [
            TriviaKind::ByteOrderMark,
            TriviaKind::Shebang,
            TriviaKind::LineBreak
        ]
    );
    assert_eq!(
        text(source, lexed.trivia()[1].span),
        b"#!/usr/bin/env lunaferra"
    );
    assert_eq!(lexed.tokens()[0].kind, T::Name);
    assert_eq!(lexed.lines().line_col(lexed.tokens()[0].span.start), (2, 1));
}

#[test]
fn skips_a_byte_order_mark_alone_or_a_first_line_alone() {
    let lexed_mark = lexed_file(b"\xEF\xBB\xBFx");
    assert_eq!(lexed_mark.trivia()[0].kind, TriviaKind::ByteOrderMark);
    assert_eq!(lexed_mark.tokens()[0].kind, T::Name);
    let lexed_line = lexed_file(b"#");
    assert_eq!(lexed_line.trivia()[0].kind, TriviaKind::Shebang);
    assert_eq!(lexed_line.tokens().len(), 1);
}

#[test]
fn reads_a_partial_byte_order_mark_as_bytes() {
    assert_eq!(
        lexed_file(b"\xEF\xBBx")
            .tokens()
            .iter()
            .map(|t| t.kind)
            .collect::<Vec<_>>(),
        [T::Unknown, T::Unknown, T::Name, T::Eof]
    );
}

#[test]
fn counts_lines_after_a_first_line_with_a_carriage_return_as_puc_rio_does() {
    // PUC-Rio skips the first line up to its first `\n`, so `cr.lua` holding these bytes gave
    // `cr.lua:2: malformed number near '0x'`, and a file with only `\r` line breaks after `#!`
    // was all one comment.
    let lexed = lexed_file(b"#!x\ry\nx = 0x");
    assert_eq!(
        report(&lexed.errors()[0]),
        b"input:2: malformed number near '0x'"
    );
    let all_comment = lexed_file(b"#!lua\rprint(1)\r");
    assert_eq!(all_comment.tokens().len(), 1);
    assert_eq!(all_comment.trivia()[0].kind, TriviaKind::Shebang);
}

#[test]
fn reads_a_strings_first_bytes_as_code() {
    assert_eq!(kinds(b"#!x"), [T::Hash, T::Unknown, T::Name, T::Eof]);
    assert_eq!(
        kinds(b"\xEF\xBB\xBF"),
        [T::Unknown, T::Unknown, T::Unknown, T::Eof]
    );
}

#[test]
fn rejects_a_source_one_byte_longer_than_the_limit() {
    // As in `tests/source.rs`, the zeroed memory is mapped lazily, and the lexer checks the
    // length before it reads a byte. Only the trusted limits reach this length. The source is a
    // file whose first line, which starts with `#`, is all of it, so a lexer that failed to refuse
    // it would read it as one skipped line, not as 4 GB of tokens that exhaust memory.
    let len = usize::try_from(u32::MAX).unwrap();
    let mut source = vec![0_u8; len];
    source[0] = b'#';
    assert_eq!(
        lex(&source, SourceId::new(0), Origin::File, &Limits::trusted()),
        Err(Error::SourceTooLong {
            len,
            limit: MAX_SOURCE_LEN
        })
    );
}

// f-strings

#[test]
fn splits_an_f_string_into_text_and_fields() {
    assert_eq!(
        tokens(r#"f"a{x}b""#),
        owned(&[
            (T::FStringStart, "f\""),
            (T::FStringText, "a"),
            (T::FStringFieldStart, "{"),
            (T::Name, "x"),
            (T::FStringFieldEnd, "}"),
            (T::FStringText, "b"),
            (T::FStringEnd, "\""),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn lexes_a_conversion_and_a_spec() {
    assert_eq!(
        tokens("f'{x!r:>8}'"),
        owned(&[
            (T::FStringStart, "f'"),
            (T::FStringFieldStart, "{"),
            (T::Name, "x"),
            (T::FStringConversion, "!r"),
            (T::FStringSpec, ":"),
            (T::FStringText, ">8"),
            (T::FStringFieldEnd, "}"),
            (T::FStringEnd, "'"),
            (T::Eof, ""),
        ])
    );
    assert_eq!(
        kinds(br#"f"{x!s}""#),
        [
            T::FStringStart,
            T::FStringFieldStart,
            T::Name,
            T::FStringConversion,
            T::FStringFieldEnd,
            T::FStringEnd,
            T::Eof
        ]
    );
}

#[test]
fn reads_a_bang_that_starts_no_conversion_as_an_operator_or_a_byte() {
    use TokenKind::*;
    let field = |inner: &[TokenKind]| {
        [
            &[FStringStart, FStringFieldStart][..],
            inner,
            &[FStringFieldEnd, FStringEnd, Eof],
        ]
        .concat()
    };
    assert_eq!(kinds(br#"f"{a!=b}""#), field(&[Name, BangEqual, Name]));
    assert_eq!(kinds(br#"f"{a!x}""#), field(&[Name, Unknown, Name]));
    assert_eq!(kinds(br#"f"{a!rx}""#), field(&[Name, Unknown, Name]));
    assert_eq!(
        kinds(br#"f"{(a!r)}""#),
        field(&[LeftParen, Name, Unknown, Name, RightParen])
    );
}

#[test]
fn lexes_a_long_f_string() {
    assert_eq!(
        tokens("f[[{x}]]"),
        owned(&[
            (T::FStringStart, "f[["),
            (T::FStringFieldStart, "{"),
            (T::Name, "x"),
            (T::FStringFieldEnd, "}"),
            (T::FStringEnd, "]]"),
            (T::Eof, ""),
        ])
    );
    assert_eq!(
        tokens("f[==[\na]]\n{t[a[1]]}]=]]==]"),
        owned(&[
            (T::FStringStart, "f[==[\n"),
            (T::FStringText, "a]]\n"),
            (T::FStringFieldStart, "{"),
            (T::Name, "t"),
            (T::LeftBracket, "["),
            (T::Name, "a"),
            (T::LeftBracket, "["),
            (T::Integer, "1"),
            (T::RightBracket, "]"),
            (T::RightBracket, "]"),
            (T::FStringFieldEnd, "}"),
            (T::FStringText, "]=]"),
            (T::FStringEnd, "]==]"),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn keeps_doubled_braces_as_text() {
    assert_eq!(
        tokens(r#"f"{{n}} is {n}""#),
        owned(&[
            (T::FStringStart, "f\""),
            (T::FStringText, "{{n}} is "),
            (T::FStringFieldStart, "{"),
            (T::Name, "n"),
            (T::FStringFieldEnd, "}"),
            (T::FStringEnd, "\""),
            (T::Eof, ""),
        ])
    );
    assert_eq!(
        tokens("f[[{{]]"),
        owned(&[
            (T::FStringStart, "f[["),
            (T::FStringText, "{{"),
            (T::FStringEnd, "]]"),
            (T::Eof, "")
        ])
    );
}

#[test]
fn reads_f_with_a_space_before_a_string_as_a_name_and_a_string() {
    assert_eq!(
        tokens(r#"f "x" f'y' f [[z]]"#),
        owned(&[
            (T::Name, "f"),
            (T::String, "\"x\""),
            (T::FStringStart, "f'"),
            (T::FStringText, "y"),
            (T::FStringEnd, "'"),
            (T::Name, "f"),
            (T::String, "[[z]]"),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn reads_f_before_a_bracket_that_opens_no_long_string_as_a_name() {
    assert_eq!(
        kinds(b"f[x] f[=x"),
        [
            T::Name,
            T::LeftBracket,
            T::Name,
            T::RightBracket,
            T::Name,
            T::Error,
            T::Name,
            T::Eof
        ]
    );
}

#[test]
fn reads_a_name_and_a_parenthesis_after_a_colon_as_a_method_call() {
    assert_eq!(
        kinds(br#"f"{p:length()}""#),
        [
            T::FStringStart,
            T::FStringFieldStart,
            T::Name,
            T::Colon,
            T::Name,
            T::LeftParen,
            T::RightParen,
            T::FStringFieldEnd,
            T::FStringEnd,
            T::Eof
        ]
    );
}

#[test]
fn reads_any_other_colon_at_a_fields_top_level_as_a_spec() {
    for (source, spec) in [
        (r#"f"{x:>8}""#, ">8"),
        (r#"f"{x:upper ()}""#, "upper ()"),
        (r#"f"{x:end()}""#, "end()"),
        (r#"f"{x::>8}""#, ":>8"),
        (r#"f"{x:1(}""#, "1("),
        (r#"f"{x:_}""#, "_"),
    ] {
        assert_eq!(
            tokens(source),
            owned(&[
                (T::FStringStart, "f\""),
                (T::FStringFieldStart, "{"),
                (T::Name, "x"),
                (T::FStringSpec, ":"),
                (T::FStringText, spec),
                (T::FStringFieldEnd, "}"),
                (T::FStringEnd, "\""),
                (T::Eof, ""),
            ]),
            "{source}"
        );
    }
}

#[test]
fn reads_a_colon_inside_brackets_of_a_field_as_code() {
    use TokenKind::*;
    assert_eq!(
        kinds(br#"f"{t[a:b()]}{ {1} }{(x):y}""#),
        [
            FStringStart,
            FStringFieldStart,
            Name,
            LeftBracket,
            Name,
            Colon,
            Name,
            LeftParen,
            RightParen,
            RightBracket,
            FStringFieldEnd,
            FStringFieldStart,
            LeftBrace,
            Integer,
            RightBrace,
            FStringFieldEnd,
            FStringFieldStart,
            LeftParen,
            Name,
            RightParen,
            FStringSpec,
            FStringText,
            FStringFieldEnd,
            FStringEnd,
            Eof
        ]
    );
    assert_eq!(
        kinds(br#"f"{t[":"]}""#),
        [
            FStringStart,
            FStringFieldStart,
            Name,
            LeftBracket,
            String,
            RightBracket,
            FStringFieldEnd,
            FStringEnd,
            Eof
        ]
    );
}

#[test]
fn reads_a_field_inside_a_spec() {
    assert_eq!(
        tokens(r#"f"{x:>{w}.2f}""#),
        owned(&[
            (T::FStringStart, "f\""),
            (T::FStringFieldStart, "{"),
            (T::Name, "x"),
            (T::FStringSpec, ":"),
            (T::FStringText, ">"),
            (T::FStringFieldStart, "{"),
            (T::Name, "w"),
            (T::FStringFieldEnd, "}"),
            (T::FStringText, ".2f"),
            (T::FStringFieldEnd, "}"),
            (T::FStringEnd, "\""),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn reads_an_f_string_inside_a_field() {
    assert_eq!(
        tokens(r#"f"{f"{x}" + 'y'}""#),
        owned(&[
            (T::FStringStart, "f\""),
            (T::FStringFieldStart, "{"),
            (T::FStringStart, "f\""),
            (T::FStringFieldStart, "{"),
            (T::Name, "x"),
            (T::FStringFieldEnd, "}"),
            (T::FStringEnd, "\""),
            (T::Plus, "+"),
            (T::String, "'y'"),
            (T::FStringFieldEnd, "}"),
            (T::FStringEnd, "\""),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn keeps_escapes_in_short_f_string_text_and_starts_no_field_at_an_escaped_brace() {
    assert_eq!(
        tokens(r#"f"\t\x7B\"{x}\n""#),
        owned(&[
            (T::FStringStart, "f\""),
            (T::FStringText, r#"\t\x7B\""#),
            (T::FStringFieldStart, "{"),
            (T::Name, "x"),
            (T::FStringFieldEnd, "}"),
            (T::FStringText, "\\n"),
            (T::FStringEnd, "\""),
            (T::Eof, ""),
        ])
    );
    assert_eq!(
        kinds(br#"f[[\q]]"#),
        [T::FStringStart, T::FStringText, T::FStringEnd, T::Eof]
    );
}

#[test]
fn keeps_spaces_in_f_string_text_as_text_and_in_a_field_as_trivia() {
    let source = b"f\" a { x }\" -- c";
    let lexed = lexed(source);
    let tokens = lexed.tokens();
    let texts: Vec<&[u8]> = tokens
        .iter()
        .map(|token| text(source, token.span))
        .collect();
    assert_eq!(texts, [&b"f\""[..], b" a ", b"{", b"x", b"}", b"\"", b""]);
    assert!(lexed.trailing(&tokens[0]).is_empty());
    assert!(lexed.trailing(&tokens[1]).is_empty());
    assert_eq!(lexed.trailing(&tokens[2]).len(), 1);
    assert_eq!(lexed.trailing(&tokens[3]).len(), 1);
    assert!(lexed.trailing(&tokens[4]).is_empty());
    assert_eq!(lexed.trailing(&tokens[5]).len(), 2);
}

#[test]
fn lets_a_field_of_a_short_f_string_span_lines() {
    assert_eq!(
        tokens("f\"{x + -- a comment\n  y}\""),
        owned(&[
            (T::FStringStart, "f\""),
            (T::FStringFieldStart, "{"),
            (T::Name, "x"),
            (T::Plus, "+"),
            (T::Name, "y"),
            (T::FStringFieldEnd, "}"),
            (T::FStringEnd, "\""),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn ends_an_f_string_at_its_quote_inside_a_spec_for_the_parser_to_report() {
    let lexed = lexed(br#"f"{x:>8" y"#);
    let kinds: Vec<TokenKind> = lexed.tokens().iter().map(|token| token.kind).collect();
    assert_eq!(
        kinds,
        [
            T::FStringStart,
            T::FStringFieldStart,
            T::Name,
            T::FStringSpec,
            T::FStringText,
            T::FStringEnd,
            T::Name,
            T::Eof
        ]
    );
    assert!(lexed.errors().is_empty());
}

#[test]
fn counts_brackets_that_close_without_opening_in_a_field_as_none() {
    assert_eq!(
        kinds(br#"f"{)]}""#),
        [
            T::FStringStart,
            T::FStringFieldStart,
            T::RightParen,
            T::RightBracket,
            T::FStringFieldEnd,
            T::FStringEnd,
            T::Eof
        ]
    );
}

/// Each broken f-string, with the message Lunaferra gives for it, as `language.md` records.
#[test]
fn reports_errors_in_f_strings() {
    let cases: [(&[u8], &[u8]); 13] = [
        (b"f\"a}b\"", b"input:1: single '}' in f-string near 'f\"a}'"),
        (b"f\"{x}}\"", b"input:1: single '}' in f-string near '}}'"),
        (b"f[[a}b]]", b"input:1: single '}' in f-string near 'f[[a}'"),
        (
            b"f\"a\\qb{x}\"",
            b"input:1: invalid escape sequence near 'f\"a\\q'",
        ),
        (
            b"f\"{x:>8\\q}\"",
            b"input:1: invalid escape sequence near ':>8\\q'",
        ),
        (b"f\"abc\nx", b"input:1: unfinished string near 'f\"abc'"),
        (b"f\"{x}abc\r\nx", b"input:1: unfinished string near '}abc'"),
        (b"f\"{x:>8\nx", b"input:1: unfinished string near ':>8'"),
        (b"f\"abc", b"input:1: unfinished string near <eof>"),
        (b"f\"{x", b"input:1: unfinished string near <eof>"),
        (b"f'{x:", b"input:1: unfinished string near <eof>"),
        (
            b"f[[abc",
            b"input:1: unfinished long string (starting at line 1) near <eof>",
        ),
        (
            b"\nf[=[{x:\n",
            b"input:3: unfinished long string (starting at line 2) near <eof>",
        ),
    ];
    for (source, message) in cases {
        assert_eq!(
            String::from_utf8_lossy(&first_error(source)),
            String::from_utf8_lossy(message),
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

#[test]
fn reports_one_error_in_each_stretch_of_f_string_text() {
    let one = lexed(b"f\"} } \\q\n");
    assert_eq!(one.errors().len(), 1);
    let two = lexed(b"f\"}{x}}\\q\"");
    let messages: Vec<String> = two.errors().iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        [
            "single '}' in f-string near 'f\"}'",
            "single '}' in f-string near '}}'",
        ]
    );
    let after_escape = lexed(b"f\"\\q");
    assert_eq!(after_escape.errors().len(), 1);
}

#[test]
fn ends_an_f_string_at_its_quote_after_a_failed_escape() {
    assert_eq!(
        tokens(r#"f"\x" y"#),
        owned(&[
            (T::FStringStart, "f\""),
            (T::Error, "\\x"),
            (T::FStringEnd, "\""),
            (T::Name, "y"),
            (T::Eof, ""),
        ])
    );
}

#[test]
fn goes_on_after_an_unfinished_f_string_on_the_next_line() {
    assert_eq!(
        tokens("f\"abc\nx = 1"),
        owned(&[
            (T::FStringStart, "f\""),
            (T::Error, "abc"),
            (T::Name, "x"),
            (T::Equal, "="),
            (T::Integer, "1"),
            (T::Eof, ""),
        ])
    );
    assert_eq!(
        tokens("f\"\nx"),
        owned(&[(T::FStringStart, "f\""), (T::Name, "x"), (T::Eof, "")])
    );
}

#[test]
fn goes_on_in_the_outer_field_after_an_unfinished_inner_f_string() {
    let lexed = lexed(b"f\"{f\"a\n}\"");
    let kinds: Vec<TokenKind> = lexed.tokens().iter().map(|token| token.kind).collect();
    assert_eq!(
        kinds,
        [
            T::FStringStart,
            T::FStringFieldStart,
            T::FStringStart,
            T::Error,
            T::FStringFieldEnd,
            T::FStringEnd,
            T::Eof
        ]
    );
    assert_eq!(lexed.errors().len(), 1);
}

#[test]
fn marks_the_text_of_an_unfinished_f_string_as_an_error() {
    assert_eq!(
        tokens("f[[ab"),
        owned(&[(T::FStringStart, "f[["), (T::Error, "ab"), (T::Eof, "")])
    );
    let lexed = lexed(b"f\"a}b");
    assert_eq!(lexed.errors().len(), 1);
    assert_eq!(lexed.errors()[0].kind, LexErrorKind::SingleClosingBrace);
}

#[test]
fn spans_an_unfinished_f_string_from_its_start() {
    let lexed = lexed(b"x = f\"{y");
    let error = &lexed.errors()[0];
    assert_eq!((error.span.start, error.span.end), (4, 8));
}

#[test]
fn lexes_deeply_nested_f_strings_without_recursion() {
    let depth = 100_000;
    let mut source = b"f\"{".repeat(depth);
    source.extend(b"}\"".repeat(depth));
    let lexed = lexed(&source);
    assert!(lexed.errors().is_empty());
    assert_eq!(lexed.tokens().len(), 4 * depth + 1);
    assert_eq!(round_trip(&lexed, &source), source);
}

/// Bytes that start or end tokens and trivia, which generated sources are made of, so that
/// most sources hold strings, comments, numerals, f-strings, and errors.
const ALPHABET: &[u8] = b"af0x9ep.\"'[]=-{}:!\\\n\r #()zurs;\x00\xFF";

#[test]
fn gives_back_every_generated_source_from_its_tokens_and_trivia() {
    Config::default().with_cases(5_000).assert(
        |source| {
            let mut bytes = Vec::new();
            while source.below(40) != 0 {
                let bound = u64::try_from(ALPHABET.len()).unwrap() + 1;
                let choice = usize::try_from(source.below(bound)).unwrap();
                let byte = match ALPHABET.get(choice) {
                    Some(&byte) => byte,
                    None => u8::try_from(source.below(256)).unwrap(),
                };
                bytes.push(byte);
            }
            bytes
        },
        |bytes| {
            for origin in [Origin::String, Origin::File] {
                let lexed = lex(bytes, SourceId::new(0), origin, &Limits::default()).unwrap();
                if round_trip(&lexed, bytes) != *bytes {
                    return Err(format!("{origin:?} round trip"));
                }
                let last = lexed.tokens().last().unwrap();
                let len = u32::try_from(bytes.len()).unwrap();
                if last.kind != T::Eof || last.span.start != len {
                    return Err(format!("{origin:?} last token {last:?}"));
                }
                for error in lexed.errors() {
                    if error.span.start > error.span.end || error.span.end > len {
                        return Err(format!("{origin:?} error span {error:?}"));
                    }
                }
            }
            Ok(())
        },
    );
}

#[test]
fn lexes_every_conformance_script_and_its_twin_without_an_error() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
    let mut scripts = 0;
    for folder in [folder.clone(), folder.join("reference")] {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "lua") {
                let source = std::fs::read(&path).unwrap();
                let lexed = lexed_file(&source);
                assert_eq!(lexed.errors(), [], "{}", path.display());
                assert_eq!(round_trip(&lexed, &source), source, "{}", path.display());
                scripts += 1;
            }
        }
    }
    assert!(scripts >= 4, "found {scripts} scripts");
}
