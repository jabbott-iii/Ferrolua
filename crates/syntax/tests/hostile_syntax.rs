//! Hostile-input tests of the lexer and the parser: the limit on a source's length, the limit on
//! nesting, which stops each construct one level past PUC-Rio Lua 5.5.1's limit with its message,
//! input nested far past that limit, and the time to parse inputs of every kind as they grow.
//!
//! Each count of levels in `PUC_RIO_LIMITS` is the deepest nesting that `load(source, "=input")`
//! accepted for the same source, called from a script's main chunk, on PUC-Rio Lua 5.5.1 on
//! 2026-11-26. One level more gave `C stack overflow`.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use lunaferra_syntax::{
    Code, CompactTree, Error, Event, Lexed, Limits, MAX_DEPTH, MAX_SOURCE_LEN, NodeKind, Origin,
    ParseError, ParseErrorKind, SourceId, SyntaxTree, Tool, diagnostics, lex, parse,
};

/// Lexes `source` as a string, under the default limits.
fn lexed(source: &[u8]) -> Lexed {
    lex(source, SourceId::new(0), Origin::String, &Limits::default())
        .expect("each source is within the default limit")
}

/// Parses `source`, checks that its events make a tree that gives back the source, and returns
/// its syntax errors.
fn errors(source: &[u8]) -> Vec<ParseError> {
    let lexed = lexed(source);
    let events = parse(source, &lexed);
    let tree = SyntaxTree::build(source, &lexed, &events).expect("the parser's events make a tree");
    assert!(
        tree.root().text() == source,
        "the tree gives back the source"
    );
    events
        .into_iter()
        .filter_map(|event| match event {
            Event::Error(error) => Some(error),
            _ => None,
        })
        .collect()
}

/// A construct nested `n` levels deep: `start`, then `open` `n` times, `middle`, and `close` `n`
/// times.
struct Nested {
    name: &'static str,
    start: &'static str,
    open: &'static str,
    middle: &'static str,
    close: &'static str,
}

impl Nested {
    fn source(&self, n: usize) -> String {
        format!(
            "{}{}{}{}",
            self.start,
            self.open.repeat(n),
            self.middle,
            self.close.repeat(n)
        )
    }
}

/// Constructs of the syntax that Lunaferra keeps from Lua 5.5, with the deepest nesting of each
/// that PUC-Rio Lua 5.5.1's `load` accepted. Lua's `^` is written `**` here. Each count is 197
/// levels in all, counted as `MAX_DEPTH` says: a `return` and its first `subexpr` take two, and a
/// function expression in a `return` takes two more.
const PUC_RIO_LIMITS: [(Nested, usize); 19] = [
    (nested("parentheses", "return ", "(", "1", ")"), 195),
    (nested("do blocks", "", "do ", "", "end "), 197),
    (
        nested("returns in blocks", "", "do ", "return ", "end "),
        196,
    ),
    (nested("while loops", "", "while x do ", "", "end "), 196),
    (nested("if statements", "", "if x then ", "", "end "), 196),
    (nested("repeat loops", "", "repeat ", "", "until x "), 196),
    (nested("tables", "return ", "{", "", "}"), 196),
    (nested("table fields", "return ", "{x=", "1", "}"), 195),
    (
        nested("functions", "return ", "function() return ", "1", " end"),
        97,
    ),
    (
        nested("function statements", "", "function f() ", "", "end "),
        197,
    ),
    (
        nested("local functions", "", "local function f() ", "", "end "),
        197,
    ),
    (nested("unary minus", "return ", "- ", "1", ""), 195),
    (nested("not", "return ", "not ", "1", ""), 195),
    (nested("powers", "return 2", "**2", "", ""), 195),
    (nested("calls", "return ", "f(", "1", ")"), 195),
    (nested("indexes", "return ", "t[", "1", "]"), 195),
    (nested("call statements", "", "f(", "1", ")"), 196),
    (nested("assignment targets", "a", ", a", " = 1", ""), 195),
    (nested("global names", "global a", ", a", " = 1", ""), 194),
];

/// Constructs of Lunaferra's own syntax, with the deepest nesting that the rule of `MAX_DEPTH`
/// gives each: a level for each `subexpr` that reads an element or a field, and one for each
/// value after a conditional's `else`.
const OWN_LIMITS: [(Nested, usize); 4] = [
    (nested("lists", "return ", "[ ", "", "]"), 196),
    (nested("tuples", "return ", "(1, ", "1", ")"), 195),
    (nested("f-strings", "return ", "f\"{", "1", "}\""), 195),
    (
        nested("conditionals", "return 1", " if x else 1", "", ""),
        195,
    ),
];

const fn nested(
    name: &'static str,
    start: &'static str,
    open: &'static str,
    middle: &'static str,
    close: &'static str,
) -> Nested {
    Nested {
        name,
        start,
        open,
        middle,
        close,
    }
}

#[test]
fn allows_197_levels_of_nesting() {
    assert_eq!(MAX_DEPTH, 197);
}

#[test]
fn nests_each_construct_of_luas_syntax_to_puc_rios_limit_and_stops_one_level_further() {
    check_each_limit();
}

#[test]
fn nests_each_construct_to_its_limit_on_a_thread_with_a_256_kb_stack() {
    // Mobile threads and browsers give smaller stacks than desktop threads, so these cases run on
    // a stack that size too (ADR-0028). A stack that overflows ends the test process.
    let thread = thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(check_each_limit)
        .expect("a thread starts");
    if let Err(panic) = thread.join() {
        std::panic::resume_unwind(panic);
    }
}

/// Checks that each construct parses at its limit, and stops one level further with PUC-Rio's
/// message.
fn check_each_limit() {
    for (construct, limit) in PUC_RIO_LIMITS.iter().chain(&OWN_LIMITS) {
        let deepest = construct.source(*limit);
        assert_eq!(errors(deepest.as_bytes()), [], "{}", construct.name);
        let too_deep = construct.source(limit + 1);
        let first = errors(too_deep.as_bytes()).into_iter().next();
        assert_eq!(
            first.map(|error| (error.message(), error.kind)),
            Some((b"C stack overflow".to_vec(), ParseErrorKind::NestingTooDeep)),
            "{}",
            construct.name
        );
    }
}

#[test]
fn reports_c_stack_overflow_at_the_first_token_too_deep_to_read() {
    // The 196th `(` would start the 198th level, so the error is at the `1` after it.
    let source = format!("return {}1{}", "(".repeat(196), ")".repeat(196));
    let lexed = lexed(source.as_bytes());
    let events = parse(source.as_bytes(), &lexed);
    let found: Vec<&ParseError> = events
        .iter()
        .filter_map(|event| match event {
            Event::Error(error) => Some(error),
            _ => None,
        })
        .collect();
    let [error] = found.as_slice() else {
        panic!("one error, not {found:?}");
    };
    let at = u32::try_from("return ".len() + 196).expect("small");
    assert_eq!(
        (
            error.kind.clone(),
            error.span.start,
            error.span.end,
            error.line
        ),
        (ParseErrorKind::NestingTooDeep, at, at + 1, 1)
    );
    assert_eq!(error.near, None);
    assert_eq!(error.to_string(), "C stack overflow");
    let [diagnostic] = diagnostics(&lexed, &events)
        .try_into()
        .expect("one diagnostic");
    assert_eq!(diagnostic.code, Code::new(Tool::Parser, 28));
    assert_eq!(diagnostic.message, "C stack overflow");

    // A statement too deep is refused at its first token.
    let blocks = format!("{}x = 1 {}", "do ".repeat(197), "end ".repeat(197));
    let error = errors(blocks.as_bytes())
        .into_iter()
        .next()
        .expect("an error");
    let at = u32::try_from(197 * "do ".len()).expect("small");
    assert_eq!(
        (error.kind, error.span.start),
        (ParseErrorKind::NestingTooDeep, at)
    );
}

#[test]
fn goes_on_after_nesting_too_deep_at_the_next_statement() {
    let source = format!("x = {}1{}\ny = 2\n", "(".repeat(300), ")".repeat(300));
    let lexed = lexed(source.as_bytes());
    let events = parse(source.as_bytes(), &lexed);
    let kinds: Vec<ParseErrorKind> = events
        .iter()
        .filter_map(|event| match event {
            Event::Error(error) => Some(error.kind.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, [ParseErrorKind::NestingTooDeep]);
    let tree = SyntaxTree::build(source.as_bytes(), &lexed, &events).expect("a tree");
    let block = tree
        .root()
        .children()
        .find_map(|child| child.into_node())
        .expect("the chunk's block");
    let statements: Vec<NodeKind> = block
        .children()
        .filter_map(|child| child.into_node())
        .map(|node| node.kind())
        .collect();
    assert_eq!(statements, [NodeKind::AssignStat, NodeKind::AssignStat]);
    // The compact tree is built only for a source with no error.
    assert_eq!(
        CompactTree::build(source.as_bytes(), &lexed, &events),
        Err(Error::SourceHasErrors)
    );
}

#[test]
fn parses_each_construct_nested_25_000_levels_with_one_error_for_the_nesting() {
    const DEPTH: usize = 25_000;
    for (construct, _) in PUC_RIO_LIMITS.iter().chain(&OWN_LIMITS) {
        let source = construct.source(DEPTH);
        let errors = errors(source.as_bytes());
        let nesting = errors
            .iter()
            .filter(|error| error.kind == ParseErrorKind::NestingTooDeep)
            .count();
        assert_eq!(
            errors.first().map(|error| &error.kind),
            Some(&ParseErrorKind::NestingTooDeep),
            "{}",
            construct.name
        );
        // A function's `return` is a stop, so after each error the recovery goes on at the main
        // chunk's statements, where the rest of the source is a nesting of its own: one error for
        // each 100 functions.
        let expected = if construct.name == "functions" {
            250
        } else {
            1
        };
        assert_eq!(nesting, expected, "{}", construct.name);
    }
}

#[test]
fn refuses_a_source_one_byte_longer_than_the_default_limit_of_16_mb() {
    const LIMIT: usize = 16 * 1024 * 1024;
    assert_eq!(Limits::default().source_len, 16_777_216);
    let at_limit = vec![b' '; LIMIT];
    let lexed = lex(
        &at_limit,
        SourceId::new(0),
        Origin::String,
        &Limits::default(),
    )
    .expect("a source at the limit");
    assert_eq!(lexed.tokens().len(), 1);
    let past = vec![b' '; LIMIT + 1];
    assert_eq!(
        lex(&past, SourceId::new(0), Origin::File, &Limits::default()),
        Err(Error::SourceTooLong {
            len: LIMIT + 1,
            limit: 16_777_216
        })
    );
    // The trusted limits refuse only what this crate can't read.
    assert_eq!(Limits::trusted().source_len, MAX_SOURCE_LEN);
    assert!(lex(&past, SourceId::new(0), Origin::String, &Limits::trusted()).is_ok());
}

#[test]
fn refuses_a_source_one_byte_longer_than_a_hosts_limit() {
    let mut limits = Limits::default();
    limits.source_len = 0;
    assert!(lex(b"", SourceId::new(0), Origin::String, &limits).is_ok());
    assert_eq!(
        lex(b";", SourceId::new(0), Origin::String, &limits),
        Err(Error::SourceTooLong { len: 1, limit: 0 })
    );
    let error = Error::SourceTooLong { len: 9, limit: 8 };
    assert_eq!(
        error.to_string(),
        "source of 9 bytes is longer than the limit of 8 bytes"
    );
}

// As in `tests/source.rs`, the zeroed memory is mapped lazily, so this reserves 4 GB of address
// space but touches little memory, and a 32-bit target can't hold a source that long. The source
// is a file whose first line, which starts with `#`, is all of it, so a lexer that failed to
// refuse it would read it as one skipped line, not as 4 GB of tokens that exhaust memory.
#[cfg(target_pointer_width = "64")]
#[test]
fn refuses_a_source_longer_than_max_source_len_under_a_higher_limit() {
    let mut limits = Limits::default();
    limits.source_len = u32::MAX;
    let len = usize::try_from(u32::MAX).expect("a 64-bit target");
    let mut source = vec![0_u8; len];
    source[0] = b'#';
    assert_eq!(
        lex(&source, SourceId::new(0), Origin::File, &limits),
        Err(Error::SourceTooLong {
            len,
            limit: MAX_SOURCE_LEN
        })
    );
}

#[test]
fn parses_a_source_of_16_mb_at_the_limit() {
    // A long comment fills the source, so it has few tokens; a source as long of dense code
    // takes gigabytes, which M1.11 addresses.
    const LIMIT: usize = 16 * 1024 * 1024;
    let mut source = b"x = [[a\r\nb]] --[==[".to_vec();
    let close = b"]==]\n";
    source.resize(LIMIT - close.len(), b'-');
    source.extend_from_slice(close);
    assert_eq!(source.len(), LIMIT);
    let lexed = lexed(&source);
    let events = parse(&source, &lexed);
    let tree = CompactTree::build(&source, &lexed, &events).expect("a valid source");
    assert_eq!(tree[tree.block()].len(), 1);
}

/// Repeats `unit` to make a source of `len` bytes or a little less.
fn repeated(unit: &str, len: usize) -> Vec<u8> {
    let count = len / unit.len();
    unit.repeat(count).into_bytes()
}

/// Returns the shortest of `runs` times to lex and parse `source`.
fn parse_time(source: &[u8], runs: usize) -> Duration {
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            let lexed = lexed(source);
            let events = parse(source, &lexed);
            let time = start.elapsed();
            drop(events);
            time
        })
        .min()
        .expect("at least one run")
}

#[test]
fn parses_inputs_of_each_kind_in_time_proportional_to_their_size() {
    // An input 16 times as large takes about 16 times as long to parse if parsing is linear, and
    // 256 times as long if it is quadratic. On the x86-64 Linux machine where this was written,
    // each kind took 18 to 23 times as long, the rest from caches that the larger input outgrows.
    // The bound of 64 times leaves room for that and for a machine's noise, whatever its speed,
    // and still catches a quadratic step; the shortest of a few runs of each leaves out a run that
    // other tests slowed. The large input is parsed on a thread of
    // its own, and the test fails once that bound has passed, so a quadratic step fails it in
    // seconds instead of running for hours. A debug build parses about 2 MB a second on the
    // x86-64 Linux machine where this was written.
    const SMALL: usize = 64 * 1024;
    const LARGE: usize = 16 * SMALL;
    let nested = format!("x = {}1{}\n", "(".repeat(195), ")".repeat(195));
    let kinds: [(&str, Vec<u8>, Vec<u8>); 7] = [
        ("code", "x = f(a, \"s\") + t.k[1] * 2\n"),
        ("errors", "x = = 1\n"),
        ("nesting at the limit", nested.as_str()),
        ("parentheses past the limit", "("),
        ("blocks past the limit", "do "),
        ("f-strings", "x = f\"a{b}{{c}}\"\n"),
        ("long strings and comments", "x = [[s]] --[==[c]==]\n"),
    ]
    .map(|(name, unit)| (name, repeated(unit, SMALL), repeated(unit, LARGE)));
    for (name, small, large) in kinds {
        let small = parse_time(&small, 5);
        let (sender, receiver) = mpsc::channel();
        // The thread may still run when the test fails; the process ends it when it exits.
        thread::spawn(move || sender.send(parse_time(&large, 2)));
        let large = receiver.recv_timeout(small * 64);
        assert!(
            large.is_ok(),
            "{name}: {small:?} for {SMALL} bytes, and more than {:?} for {LARGE}",
            small * 64
        );
    }
}
