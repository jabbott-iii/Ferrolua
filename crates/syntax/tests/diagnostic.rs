//! Tests of the diagnostics of a source's errors, through the public API: the code, severity,
//! message, and span of the diagnostic of each kind of lexical and syntax error, the fixes for
//! Lua's operators, the order of [`source_errors`], and what a [`SourceError`] gives.

use lunaferra_syntax::{
    Code, Diagnostic, Edit, Fix, Lexed, Limits, Origin, Severity, SourceError, SourceId, Span,
    Tool, diagnostics, lex, parse, source_errors,
};

/// Lexes `source` as a string, as `load` reads one.
fn lexed(source: &[u8]) -> Lexed {
    lex(source, SourceId::new(0), Origin::String, &Limits::default())
        .expect("each test's source is short")
}

/// Returns the diagnostics of `source`.
fn diagnosed(source: &[u8]) -> Vec<Diagnostic> {
    let lexed = lexed(source);
    diagnostics(&lexed, &parse(source, &lexed))
}

/// Returns the span of `start..end` in the tests' source.
fn span(start: u32, end: u32) -> Span {
    Span {
        source: SourceId::new(0),
        start,
        end,
    }
}

/// Returns the diagnostic that a source with one error of code `P<number>` must give.
fn error(number: u16, message: &str, start: u32, end: u32) -> Diagnostic {
    let code = Code::new(Tool::Parser, number);
    Diagnostic::new(code, Severity::Error, message, span(start, end))
}

#[test]
fn gives_each_kind_of_lexical_error_its_code_message_and_span() {
    let cases: [(&[u8], Diagnostic); 12] = [
        (b"x = 0x", error(1, "malformed number near '0x'", 4, 6)),
        (b"x = \"abc", error(2, "unfinished string near <eof>", 4, 8)),
        (
            b"x = [[abc",
            error(
                3,
                "unfinished long string (starting at line 1) near <eof>",
                4,
                9,
            ),
        ),
        (
            b"--[[ x",
            error(
                4,
                "unfinished long comment (starting at line 1) near <eof>",
                0,
                6,
            ),
        ),
        (
            b"x = [==",
            error(5, "invalid long string delimiter near '[=='", 4, 7),
        ),
        (
            b"x = \"\\q\"",
            error(6, "invalid escape sequence near '\"\\q'", 4, 7),
        ),
        (
            b"x = \"\\xZZ\"",
            error(7, "hexadecimal digit expected near '\"\\xZ'", 4, 8),
        ),
        (
            b"x = \"\\u12\"",
            error(8, "missing '{' near '\"\\u1'", 4, 8),
        ),
        (
            b"x = \"\\u{12\"",
            error(9, "missing '}' near '\"\\u{12\"'", 4, 11),
        ),
        (
            b"x = \"\\u{110000000}\"",
            error(10, "UTF-8 value too large near '\"\\u{110000000'", 4, 17),
        ),
        (
            b"x = \"\\300\"",
            error(11, "decimal escape too large near '\"\\300\"'", 4, 10),
        ),
        (
            b"x = f\"}\"",
            error(12, "single '}' in f-string near 'f\"}'", 4, 7),
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(
            diagnosed(source),
            [expected],
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

#[test]
fn gives_each_kind_of_syntax_error_its_code_message_and_span() {
    // The 196th `(` would start the 198th level of nesting, at the `1`.
    let too_deep = format!("return {}1{}", "(".repeat(196), ")".repeat(196));
    let cases: [(&[u8], Diagnostic); 14] = [
        (b"if x", error(13, "'then' expected near <eof>", 4, 4)),
        (
            b"if x then\n",
            error(
                14,
                "'end' expected (to close 'if' at line 1) near <eof>",
                10,
                10,
            ),
        ),
        (b"x = = 1", error(15, "unexpected symbol near '='", 4, 5)),
        (b"x", error(16, "syntax error near <eof>", 1, 1)),
        (
            b"for i do end",
            error(17, "'=' or 'in' expected near 'do'", 6, 8),
        ),
        (
            b"function f(1) end",
            error(18, "<name> or '...' expected near '1'", 11, 12),
        ),
        (
            b"a:b",
            error(19, "function arguments expected near <eof>", 3, 3),
        ),
        (b"break", error(20, "break outside loop near 'break'", 0, 5)),
        (
            b"function f() return ... end",
            error(
                21,
                "cannot use '...' outside a vararg function near '...'",
                20,
                23,
            ),
        ),
        (
            b"local x <foo>",
            error(22, "unknown attribute 'foo'", 9, 12),
        ),
        (
            b"local a <close>, b <close>",
            error(23, "multiple to-be-closed variables in local list", 17, 18),
        ),
        (
            b"global x <close>",
            error(24, "global variables cannot be to-be-closed", 10, 15),
        ),
        // `+` binds more tightly than `..` did and joins only strings, so no fix is certain.
        (
            b"x = a .. b",
            error(25, "'..' is not an operator: use '+' to join strings", 6, 8),
        ),
        (too_deep.as_bytes(), error(28, "C stack overflow", 203, 204)),
    ];
    for (source, expected) in cases {
        assert_eq!(
            diagnosed(source),
            [expected],
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

/// Returns `diagnostic` with a fix, described by `message`, that writes `text` in place of the
/// diagnostic's span.
fn with_fix(mut diagnostic: Diagnostic, message: &str, text: &str) -> Diagnostic {
    diagnostic.fixes.push(Fix {
        message: message.to_owned(),
        edits: vec![Edit {
            span: diagnostic.span,
            text: text.as_bytes().to_vec(),
        }],
    });
    diagnostic
}

#[test]
fn offers_the_operator_its_message_names_for_luas_not_equal_and_xor() {
    let not_equal = error(
        26,
        "'~=' is not an operator: use '!=' to test inequality",
        6,
        8,
    );
    assert_eq!(
        diagnosed(b"x = a ~= b"),
        [with_fix(not_equal, "replace '~=' with '!='", "!=")]
    );
    let xor = error(
        27,
        "'~' is not a binary operator: use '^' for bitwise XOR",
        6,
        7,
    );
    assert_eq!(
        diagnosed(b"x = a ~ b"),
        [with_fix(xor, "replace '~' with '^'", "^")]
    );
}

#[test]
fn puts_a_lexical_error_before_a_syntax_error_at_the_same_token() {
    // PUC-Rio's lexer fails on `0x` while it reads the token after `>`, before its parser
    // checks the attribute.
    let source = b"local x <foo> 0x";
    let lexed = lexed(source);
    let events = parse(source, &lexed);
    let errors = source_errors(&lexed, &events);
    assert!(matches!(
        errors.as_slice(),
        [SourceError::Lex(_), SourceError::Parse(_)]
    ));
    let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        ["malformed number near '0x'", "unknown attribute 'foo'"]
    );

    // A syntax error at an earlier token comes first.
    let earlier = b"x = = 0x";
    let earlier_lexed = crate::lexed(earlier);
    let events = parse(earlier, &earlier_lexed);
    let messages: Vec<String> = source_errors(&earlier_lexed, &events)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        messages,
        ["unexpected symbol near '='", "malformed number near '0x'"]
    );
}

#[test]
fn gives_each_source_errors_message_line_span_and_diagnostic() {
    let source = b"x = 1\n'\xFF' --[[";
    let lexed = lexed(source);
    let events = parse(source, &lexed);
    let errors = source_errors(&lexed, &events);
    let [syntax, lexical] = errors.as_slice() else {
        panic!("two errors: {errors:?}");
    };
    assert_eq!(syntax.message(), b"unexpected symbol near ''\xFF''");
    assert_eq!(syntax.to_string(), "unexpected symbol near ''\u{FFFD}''");
    assert_eq!(syntax.line(), 2);
    assert_eq!(syntax.span(), span(6, 9));
    assert_eq!(
        syntax.diagnostic(),
        error(15, "unexpected symbol near ''\u{FFFD}''", 6, 9)
    );
    assert_eq!(
        lexical.message(),
        b"unfinished long comment (starting at line 2) near <eof>"
    );
    assert_eq!(lexical.line(), 2);
    assert_eq!(lexical.span(), span(10, 14));
    assert_eq!(
        lexical.diagnostic(),
        error(
            4,
            "unfinished long comment (starting at line 2) near <eof>",
            10,
            14
        )
    );
}

#[test]
fn gives_no_diagnostic_for_a_source_without_an_error() {
    assert_eq!(diagnosed(b""), []);
    assert_eq!(diagnosed(b"local x = f\"{1 + 2}\"\nprint(x)\n"), []);
}
