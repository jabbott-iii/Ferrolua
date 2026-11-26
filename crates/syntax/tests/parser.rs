//! Tests of the parser through the public API: the node kind of each statement and of each
//! expression it reads, the message and line of each syntax error near them, which error comes
//! first when a source also has a lexical error, the typed views of what it builds, and the
//! round trip from its tree back to the source on generated input.
//!
//! Each expected message is what `load(source, "=input")` gave for the same source on PUC-Rio
//! Lua 5.5.1 on 2026-11-25. Lunaferra reserves `global`, so the messages for `global` statements
//! come from a build of the same sources with `LUA_COMPAT_GLOBAL` set to 0, which reserves it
//! too; the default build gives the same messages for every other source here.

use lunaferra_syntax::ast::{
    AstNode, Chunk, Expr, FuncBody, GenericForStat, IfStat, LocalStat, NumericForStat, Stat,
};
use lunaferra_syntax::{
    Error, Event, Lexed, NodeKind, Origin, ParseError, ParseErrorKind, SourceId, Span,
    SyntaxElement, SyntaxNode, SyntaxToken, SyntaxTree, TokenKind, lex, parse,
};
use lunaferra_testkit::{Config, Source};

use NodeKind as N;
use TokenKind as T;

/// Lexes `source` as a string, as `load` reads one.
fn lexed(source: &[u8]) -> Lexed {
    lex(source, SourceId::new(0), Origin::String).expect("each test's source is short")
}

/// Parses `source` and builds its tree, which the parser's events must always make.
fn parsed(source: &[u8]) -> (SyntaxTree, Lexed, Vec<ParseError>) {
    let lexed = lexed(source);
    let events = parse(source, &lexed);
    let tree = SyntaxTree::build(source, &lexed, &events).expect("the events make a tree");
    let errors = events
        .into_iter()
        .filter_map(|event| match event {
            Event::Error(error) => Some(error),
            _ => None,
        })
        .collect();
    (tree, lexed, errors)
}

/// Returns the source's first error as PUC-Rio's `load` writes it for the chunk name `=input`:
/// `input:`, the line, `: `, and the message; or nothing, if the source has no error.
fn first_error(source: &[u8]) -> String {
    let (_, lexed, errors) = parsed(source);
    assert!(errors.len() <= 1, "the parser reports only the first error");
    if let Some(error) = errors.first() {
        return format!("input:{}: {error}", error.line);
    }
    lexed
        .errors()
        .first()
        .map(|error| format!("input:{}: {error}", error.line))
        .unwrap_or_default()
}

/// Appends `node` to `out` as an S-expression: its kind, then each child, with each token
/// written as its bytes and the end of the source as `<eof>`.
fn write_shape(node: SyntaxNode<'_>, out: &mut String) {
    out.push('(');
    out.push_str(&format!("{:?}", node.kind()));
    for child in node.children() {
        out.push(' ');
        match child {
            SyntaxElement::Node(node) => write_shape(node, out),
            SyntaxElement::Token(token) if token.kind() == T::Eof => out.push_str("<eof>"),
            SyntaxElement::Token(token) => out.push_str(&String::from_utf8_lossy(token.text())),
        }
    }
    out.push(')');
}

/// Returns the shape of each child of the chunk's block, separated by spaces, whether or not the
/// source has an error.
fn block_shape(source: &str) -> String {
    let (tree, _, _) = parsed(source.as_bytes());
    let root = tree.root();
    let block = root
        .children()
        .find_map(SyntaxElement::into_node)
        .expect("the chunk holds a block");
    let mut shapes = Vec::new();
    for child in block.children() {
        let mut out = String::new();
        match child {
            SyntaxElement::Node(node) => write_shape(node, &mut out),
            SyntaxElement::Token(token) => out.push_str(&String::from_utf8_lossy(token.text())),
        }
        shapes.push(out);
    }
    shapes.join(" ")
}

/// Returns the shape of each statement of `source`, which must parse without an error.
fn shape(source: &str) -> String {
    assert_eq!(first_error(source.as_bytes()), "", "{source:?}");
    block_shape(source)
}

/// Returns the bytes of `token`, as text.
fn text(token: Option<SyntaxToken<'_>>) -> String {
    token.map_or_else(String::new, |token| {
        String::from_utf8_lossy(token.text()).into_owned()
    })
}

/// Returns the source text of an expression.
fn expr_text(expr: Option<Expr<'_>>) -> String {
    expr.map_or_else(String::new, |expr| {
        String::from_utf8_lossy(&expr.syntax().text())
            .trim()
            .to_owned()
    })
}

#[test]
fn makes_a_chunk_of_the_block_and_the_end_of_the_source() {
    let (tree, _, _) = parsed(b"x = 1\n");
    let mut out = String::new();
    write_shape(tree.root(), &mut out);
    assert_eq!(
        out,
        "(Chunk (Block (AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1)))) <eof>)"
    );
    let (tree, _, errors) = parsed(b"");
    let mut out = String::new();
    write_shape(tree.root(), &mut out);
    assert_eq!(out, "(Chunk (Block) <eof>)");
    assert!(errors.is_empty());
}

#[test]
fn parses_each_statement_to_its_node_kind() {
    let cases = [
        (";", "(EmptyStat ;)"),
        (
            "a, t.b, t[1] = 1, 2",
            "(AssignStat (ExprList (NameExpr a) , (FieldExpr (NameExpr t) . b) , \
             (IndexExpr (NameExpr t) [ (LiteralExpr 1) ])) = \
             (ExprList (LiteralExpr 1) , (LiteralExpr 2)))",
        ),
        (
            "f(x)",
            "(CallStat (CallExpr (NameExpr f) (Args ( (ExprList (NameExpr x)) ))))",
        ),
        (
            "o:m'x'",
            "(CallStat (MethodCallExpr (NameExpr o) : m (Args 'x')))",
        ),
        ("::top::", "(LabelStat :: top ::)"),
        ("goto top", "(GotoStat goto top)"),
        ("do end", "(DoStat do (Block) end)"),
        (
            "while w do break end",
            "(WhileStat while (NameExpr w) do (Block (BreakStat break)) end)",
        ),
        (
            "repeat break until r",
            "(RepeatStat repeat (Block (BreakStat break)) until (NameExpr r))",
        ),
        (
            "if a then elseif b then elseif c then else end",
            "(IfStat if (NameExpr a) then (Block) (ElseifClause elseif (NameExpr b) then (Block)) \
             (ElseifClause elseif (NameExpr c) then (Block)) (ElseClause else (Block)) end)",
        ),
        (
            "for i = 0, 10 do end",
            "(NumericForStat for i = (LiteralExpr 0) , (LiteralExpr 10) do (Block) end)",
        ),
        (
            "for i = 0, 10, 2 do break end",
            "(NumericForStat for i = (LiteralExpr 0) , (LiteralExpr 10) , (LiteralExpr 2) do \
             (Block (BreakStat break)) end)",
        ),
        (
            "for k, v in next, t do end",
            "(GenericForStat for (NameList k , v) in (ExprList (NameExpr next) , (NameExpr t)) \
             do (Block) end)",
        ),
        (
            "for k in t do end",
            "(GenericForStat for (NameList k) in (ExprList (NameExpr t)) do (Block) end)",
        ),
        (
            "function a.b:c(x, ...) end",
            "(FunctionStat function (FuncName a . b : c) (FuncBody (ParamList ( (Param x) , \
             (VarargParam ...) )) (Block) end))",
        ),
        (
            "local function f() end",
            "(LocalFunctionStat local function f (FuncBody (ParamList ( )) (Block) end))",
        ),
        (
            "global function g(...args) end",
            "(GlobalFunctionStat global function g (FuncBody (ParamList ( (VarargParam ... args) \
             )) (Block) end))",
        ),
        ("local x", "(LocalStat local (AttNameList (AttName x)))"),
        (
            "local <const> x, y <close> = 1, 2",
            "(LocalStat local (AttNameList (Attrib < const >) (AttName x) , \
             (AttName y (Attrib < close >))) = (ExprList (LiteralExpr 1) , (LiteralExpr 2)))",
        ),
        ("global *", "(GlobalStat global *)"),
        (
            "global <const> *",
            "(GlobalStat global (Attrib < const >) *)",
        ),
        (
            "global <const> x, y",
            "(GlobalStat global (AttNameList (Attrib < const >) (AttName x) , (AttName y)))",
        ),
        (
            "global x <const> = 1",
            "(GlobalStat global (AttNameList (AttName x (Attrib < const >))) = \
             (ExprList (LiteralExpr 1)))",
        ),
        ("nonlocal n", "(NonlocalStat nonlocal (NameList n))"),
        ("nonlocal a, b", "(NonlocalStat nonlocal (NameList a , b))"),
        ("return", "(ReturnStat return)"),
        ("return;", "(ReturnStat return ;)"),
        (
            "return 1, 2;",
            "(ReturnStat return (ExprList (LiteralExpr 1) , (LiteralExpr 2)) ;)",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(shape(source), expected, "{source:?}");
    }
}

#[test]
fn parses_a_sequence_of_statements_in_one_block() {
    assert_eq!(
        shape("x = 1; f() return x"),
        "(AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1))) (EmptyStat ;) \
         (CallStat (CallExpr (NameExpr f) (Args ( )))) (ReturnStat return (ExprList (NameExpr x)))"
    );
    assert_eq!(
        shape("do return end x = 1"),
        "(DoStat do (Block (ReturnStat return)) end) \
         (AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1)))"
    );
}

#[test]
fn parses_each_expression_that_statements_are_built_from() {
    let cases = [
        ("x = nil", "(LiteralExpr nil)"),
        ("x = true", "(LiteralExpr true)"),
        ("x = false", "(LiteralExpr false)"),
        ("x = 42", "(LiteralExpr 42)"),
        ("x = 1.5e3", "(LiteralExpr 1.5e3)"),
        ("x = [[long]]", "(LiteralExpr [[long]])"),
        ("x = ...", "(VarargExpr ...)"),
        ("x = y", "(NameExpr y)"),
        ("x = (y)", "(ParenExpr ( (NameExpr y) ))"),
        ("x = a.b.c", "(FieldExpr (FieldExpr (NameExpr a) . b) . c)"),
        (
            "x = t[k][1]",
            "(IndexExpr (IndexExpr (NameExpr t) [ (NameExpr k) ]) [ (LiteralExpr 1) ])",
        ),
        (
            "x = f()(1, 2)",
            "(CallExpr (CallExpr (NameExpr f) (Args ( ))) (Args ( (ExprList (LiteralExpr 1) , \
             (LiteralExpr 2)) )))",
        ),
        ("x = f 's'", "(CallExpr (NameExpr f) (Args 's'))"),
        // `f` touching a string starts an f-string (ADR-0025), so this call uses `g`.
        ("x = g[[s]]", "(CallExpr (NameExpr g) (Args [[s]]))"),
        (
            "x = s:upper():lower 'x'",
            "(MethodCallExpr (MethodCallExpr (NameExpr s) : upper (Args ( ))) : lower (Args 'x'))",
        ),
        (
            "x = (f).y(...)[1]",
            "(IndexExpr (CallExpr (FieldExpr (ParenExpr ( (NameExpr f) )) . y) \
             (Args ( (ExprList (VarargExpr ...)) ))) [ (LiteralExpr 1) ])",
        ),
    ];
    for (source, expected) in cases {
        let statement = shape(source);
        let values = statement
            .strip_prefix("(AssignStat (ExprList (NameExpr x)) = (ExprList ")
            .and_then(|rest| rest.strip_suffix("))"))
            .unwrap_or_else(|| panic!("{source:?} is one assignment: {statement}"));
        assert_eq!(values, expected, "{source:?}");
    }
}

#[test]
fn continues_an_expression_on_the_next_line() {
    // As in Lua, a `(` or a string at the start of a line continues the call before it.
    assert_eq!(
        shape("f\n(1)"),
        "(CallStat (CallExpr (NameExpr f) (Args ( (ExprList (LiteralExpr 1)) ))))"
    );
    assert_eq!(
        shape("x = y\n(f)()"),
        "(AssignStat (ExprList (NameExpr x)) = (ExprList (CallExpr (CallExpr (NameExpr y) \
         (Args ( (ExprList (NameExpr f)) ))) (Args ( )))))"
    );
    // A literal takes no call, so `(f)()` starts a statement of its own.
    assert_eq!(
        shape("x = 1\n(f)()"),
        "(AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1))) \
         (CallStat (CallExpr (ParenExpr ( (NameExpr f) )) (Args ( ))))"
    );
}

#[test]
fn reads_nonlocal_as_a_keyword_only_before_a_name() {
    assert_eq!(
        shape("nonlocal = 1"),
        "(AssignStat (ExprList (NameExpr nonlocal)) = (ExprList (LiteralExpr 1)))"
    );
    assert_eq!(
        shape("nonlocal.x = 1"),
        "(AssignStat (ExprList (FieldExpr (NameExpr nonlocal) . x)) = (ExprList (LiteralExpr 1)))"
    );
    assert_eq!(
        shape("nonlocal(x)"),
        "(CallStat (CallExpr (NameExpr nonlocal) (Args ( (ExprList (NameExpr x)) ))))"
    );
    assert_eq!(
        shape("nonlocal 'x'"),
        "(CallStat (CallExpr (NameExpr nonlocal) (Args 'x')))"
    );
    // Only at the start of a statement.
    assert_eq!(
        shape("x = nonlocal"),
        "(AssignStat (ExprList (NameExpr x)) = (ExprList (NameExpr nonlocal)))"
    );
    assert_eq!(first_error(b"nonlocal"), "input:1: syntax error near <eof>");
    assert_eq!(
        first_error(b"nonlocal x,"),
        "input:1: <name> expected near <eof>"
    );
    assert_eq!(
        first_error(b"nonlocal x y"),
        "input:1: syntax error near <eof>"
    );
}

#[test]
fn accepts_what_puc_rio_accepts() {
    let sources: [&[u8]; 39] = [
        b"local <const> x <close>",
        b"local x <close> = 1",
        b"local x <const> = 1",
        b"local x <\n\nconst\n>",
        b"global *",
        b"global <const> *",
        b"global x = 1",
        b"global x <const> = 1",
        b"function f(...) return ... end",
        b"function f(...t) return ... end",
        b"function f(a, b, ...) end",
        b"function f(a, ...b) end",
        b"function a.b.c:d() end",
        b"x = ...",
        b"f(...)",
        b"repeat break until x",
        b"for i = 0, 1 do break end",
        b"for k in x do break end",
        b"while x do local function f() end break end",
        b"x.y, z[1], f().g = 1, 2, 3",
        // `f` touching a string starts an f-string (ADR-0025), so these calls use `g`.
        b"g'a''b'",
        b"g[[x]]",
        b"g'a'.b = 1",
        b"x:y'1'",
        b"x:y()()",
        b"(f)()",
        b"g''",
        b":: x ::",
        b"::x::\ny()",
        b"return\n",
        b"return\n;",
        b"return 1, 2 ;\n",
        b"x() return",
        b"for x, y, z in a, b, c do end",
        b"if x then elseif y then else end",
        b";;;",
        b"return x;",
        b"-- a comment\n--[[ a long\ncomment ]]\n",
        b"\xEF\xBB\xBF#!/usr/bin/env lunaferra\nx = 1",
    ];
    for source in sources {
        let source = if source.starts_with(b"\xEF") {
            let lexed = lex(source, SourceId::new(0), Origin::File).expect("short");
            let events = parse(source, &lexed);
            assert!(!events.iter().any(|event| matches!(event, Event::Error(_))));
            continue;
        } else {
            source
        };
        assert_eq!(
            first_error(source),
            "",
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

#[test]
fn gives_puc_rios_message_for_each_error_near_an_assignment_or_a_call() {
    let cases: [(&[u8], &str); 34] = [
        (b"x", "input:1: syntax error near <eof>"),
        (b"x.y", "input:1: syntax error near <eof>"),
        (b"x[1]", "input:1: syntax error near <eof>"),
        (b"(x)", "input:1: syntax error near <eof>"),
        (b"x += 1", "input:1: syntax error near '+='"),
        (b"f() = 1", "input:1: syntax error near '='"),
        (b"x, f() = 1", "input:1: syntax error near '='"),
        (b"(x), y = 1", "input:1: syntax error near ','"),
        (b"x, (y) = 1", "input:1: syntax error near '='"),
        (b"a.b:c'x' = 1", "input:1: syntax error near '='"),
        (b"x, 1 = 2", "input:1: unexpected symbol near '1'"),
        (b"x, y", "input:1: '=' expected near <eof>"),
        (b"x, y()", "input:1: syntax error near <eof>"),
        (b"x =", "input:1: unexpected symbol near <eof>"),
        (b"x = 1,", "input:1: unexpected symbol near <eof>"),
        (b"x.1", "input:1: syntax error near '.1'"),
        (b"x[", "input:1: unexpected symbol near <eof>"),
        (b"x[1", "input:1: ']' expected near <eof>"),
        (b"x[\n1", "input:2: ']' expected near <eof>"),
        (b"a.b.\n", "input:2: <name> expected near <eof>"),
        (b"f(a.\n)", "input:2: <name> expected near ')'"),
        (b"f:", "input:1: <name> expected near <eof>"),
        (b"x:1", "input:1: <name> expected near '1'"),
        (b"f:g", "input:1: function arguments expected near <eof>"),
        (b"x.y:z", "input:1: function arguments expected near <eof>"),
        (b"f:g.h", "input:1: function arguments expected near '.'"),
        (b"f(", "input:1: unexpected symbol near <eof>"),
        (b"f(1", "input:1: ')' expected near <eof>"),
        (b"f(1,", "input:1: unexpected symbol near <eof>"),
        (
            b"f(\n1",
            "input:2: ')' expected (to close '(' at line 1) near <eof>",
        ),
        (
            b"f(\n1,\n2",
            "input:3: ')' expected (to close '(' at line 1) near <eof>",
        ),
        (
            b"x = (\n1",
            "input:2: ')' expected (to close '(' at line 1) near <eof>",
        ),
        (
            b"x = (1\n",
            "input:2: ')' expected (to close '(' at line 1) near <eof>",
        ),
        (b"f(\"x\"\n,", "input:2: unexpected symbol near <eof>"),
    ];
    for (source, message) in cases {
        assert_eq!(
            first_error(source),
            message,
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

#[test]
fn writes_each_token_after_near_as_puc_rios_lexer_keeps_it() {
    let cases: [(&[u8], &str); 25] = [
        (b"@", "input:1: unexpected symbol near '@'"),
        (b"x = @", "input:1: unexpected symbol near '@'"),
        (b"x = \x7f", "input:1: unexpected symbol near '<\\127>'"),
        (b"x = \x1f", "input:1: unexpected symbol near '<\\31>'"),
        (b"x = \xc8", "input:1: unexpected symbol near '<\\200>'"),
        // PUC-Rio reads a zero byte as the token 0, which its messages leave out.
        (b"x = \0", "input:1: unexpected symbol"),
        (b"x = 1 \0", "input:1: unexpected symbol"),
        (b"\"abc\"", "input:1: unexpected symbol near '\"abc\"'"),
        (
            b"x = 1 \"a\\65\"",
            "input:1: unexpected symbol near '\"aA\"'",
        ),
        (
            b"x = 1 'a\\z   \n  b'",
            "input:2: unexpected symbol near ''ab''",
        ),
        (
            b"x = 1 \"a\\x41\\u{48}\\100\"",
            "input:1: unexpected symbol near '\"aAHd\"'",
        ),
        (b"x = 1 'a\\0b'", "input:1: unexpected symbol near ''a'"),
        (
            b"x = 1 \"\\\n\"",
            "input:2: unexpected symbol near '\"\n\"'",
        ),
        (
            b"x = 1 \"\\\r\n\"",
            "input:2: unexpected symbol near '\"\n\"'",
        ),
        (
            b"x = 1 [[\nab\ncd]]",
            "input:3: unexpected symbol near '[[ab\ncd]]'",
        ),
        (
            b"x = 1 [==[\r\nab\r\ncd\n\r]==]",
            "input:4: unexpected symbol near '[==[ab\ncd\n]==]'",
        ),
        (
            b"x = 1 [[a\rb]]",
            "input:2: unexpected symbol near '[[a\nb]]'",
        ),
        (
            b"x = 1 [[a\n\rb\r\rc]]",
            "input:4: unexpected symbol near '[[a\nb\n\nc]]'",
        ),
        (b"x = 1 [[a\0b]]", "input:1: unexpected symbol near '[[a'"),
        (b"x = 1 2", "input:1: unexpected symbol near '2'"),
        (b"x = 1 1.5e3", "input:1: unexpected symbol near '1.5e3'"),
        (b"x = 1 0x10", "input:1: unexpected symbol near '0x10'"),
        (b"x = nil nil", "input:1: unexpected symbol near 'nil'"),
        (b"x = 1 ...", "input:1: unexpected symbol near '...'"),
        (
            b"x = 1\n\n\n\n  ;  )",
            "input:5: unexpected symbol near ')'",
        ),
    ];
    for (source, message) in cases {
        assert_eq!(
            first_error(source),
            message,
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

#[test]
fn gives_puc_rios_message_for_each_error_near_a_declaration() {
    let cases: [(&[u8], &str); 31] = [
        (b"local", "input:1: <name> expected near <eof>"),
        (b"local 1", "input:1: <name> expected near '1'"),
        (b"local x = 1 local", "input:1: <name> expected near <eof>"),
        (b"local x <foo>", "input:1: unknown attribute 'foo'"),
        (
            b"local x <foo>\n\n\ny = 1",
            "input:1: unknown attribute 'foo'",
        ),
        (
            b"do\nlocal x <foo>\n\n\ny = 1 end",
            "input:2: unknown attribute 'foo'",
        ),
        (b"local x <const", "input:1: '>' expected near <eof>"),
        (
            b"local x <close>, y <close>",
            "input:1: multiple to-be-closed variables in local list",
        ),
        (
            b"local x <close>, y <close>\n\nz = 1",
            "input:1: multiple to-be-closed variables in local list",
        ),
        (
            b"local x <close>, y <close> <const>",
            "input:1: multiple to-be-closed variables in local list",
        ),
        (
            b"local <close> x, y",
            "input:1: multiple to-be-closed variables in local list",
        ),
        (
            b"local x <\n\nconst\n>, y <close>, z <close>",
            "input:4: multiple to-be-closed variables in local list",
        ),
        (b"local function", "input:1: <name> expected near <eof>"),
        (b"local function f", "input:1: '(' expected near <eof>"),
        (b"local function f.g()", "input:1: '(' expected near '.'"),
        (
            b"local function f(",
            "input:1: <name> or '...' expected near <eof>",
        ),
        (
            b"local function f\n(\n)\n",
            "input:4: 'end' expected (to close 'function' at line 2) near <eof>",
        ),
        (b"global", "input:1: <name> expected near <eof>"),
        (b"global 1", "input:1: <name> expected near '1'"),
        (b"global x,", "input:1: <name> expected near <eof>"),
        (b"global x <const", "input:1: '>' expected near <eof>"),
        (b"global <const>", "input:1: <name> expected near <eof>"),
        (b"global <const> 1", "input:1: <name> expected near '1'"),
        (
            b"global x <close>",
            "input:1: global variables cannot be to-be-closed",
        ),
        (
            b"global <close> *",
            "input:1: global variables cannot be to-be-closed",
        ),
        (b"global *, x", "input:1: unexpected symbol near ','"),
        (b"global x = ", "input:1: unexpected symbol near <eof>"),
        (b"global <foo> *", "input:1: unknown attribute 'foo'"),
        (b"global x <foo>", "input:1: unknown attribute 'foo'"),
        (b"global function f.g()", "input:1: '(' expected near '.'"),
        (
            b"global function f\n(\n)\n",
            "input:4: 'end' expected (to close 'function' at line 2) near <eof>",
        ),
    ];
    for (source, message) in cases {
        assert_eq!(
            first_error(source),
            message,
            "{}",
            String::from_utf8_lossy(source)
        );
    }
    assert_eq!(
        first_error(b"global function"),
        "input:1: <name> expected near <eof>"
    );
    assert_eq!(
        first_error(b"global function f"),
        "input:1: '(' expected near <eof>"
    );
}

#[test]
fn gives_puc_rios_message_for_each_error_near_a_jump_or_a_label() {
    let cases: [(&[u8], &str); 10] = [
        (b"goto", "input:1: <name> expected near <eof>"),
        (b"goto 1", "input:1: <name> expected near '1'"),
        (b"goto x goto", "input:1: <name> expected near <eof>"),
        (b"::", "input:1: <name> expected near <eof>"),
        (b"::x", "input:1: '::' expected near <eof>"),
        (b"::x:: ::", "input:1: <name> expected near <eof>"),
        (b"break", "input:1: break outside loop near 'break'"),
        (
            b"while x do break end break",
            "input:1: break outside loop near 'break'",
        ),
        (
            b"while true do function f() break end end",
            "input:1: break outside loop near 'break'",
        ),
        (
            b"repeat local function f() break end until 1",
            "input:1: break outside loop near 'break'",
        ),
    ];
    for (source, message) in cases {
        assert_eq!(
            first_error(source),
            message,
            "{}",
            String::from_utf8_lossy(source)
        );
    }
}

#[test]
fn gives_puc_rios_message_for_each_error_near_a_block_or_a_loop() {
    let cases: [(&[u8], &str); 38] = [
        (b"do", "input:1: 'end' expected near <eof>"),
        (
            b"do\n",
            "input:2: 'end' expected (to close 'do' at line 1) near <eof>",
        ),
        (
            b"do\nx()\n",
            "input:3: 'end' expected (to close 'do' at line 1) near <eof>",
        ),
        (b"do end do", "input:1: 'end' expected near <eof>"),
        (b"while", "input:1: unexpected symbol near <eof>"),
        (b"while x", "input:1: 'do' expected near <eof>"),
        (b"while x do", "input:1: 'end' expected near <eof>"),
        (
            b"while x\ndo\n",
            "input:3: 'end' expected (to close 'while' at line 1) near <eof>",
        ),
        (b"while x do end end", "input:1: <eof> expected near 'end'"),
        (b"repeat", "input:1: 'until' expected near <eof>"),
        (
            b"repeat\nx()",
            "input:2: 'until' expected (to close 'repeat' at line 1) near <eof>",
        ),
        (b"repeat until", "input:1: unexpected symbol near <eof>"),
        (
            b"repeat until x until",
            "input:1: <eof> expected near 'until'",
        ),
        (b"if", "input:1: unexpected symbol near <eof>"),
        (b"if x", "input:1: 'then' expected near <eof>"),
        (b"if x then", "input:1: 'end' expected near <eof>"),
        (
            b"if x then\n",
            "input:2: 'end' expected (to close 'if' at line 1) near <eof>",
        ),
        (
            b"if x then\nelse\nelseif y then end",
            "input:3: 'end' expected (to close 'if' at line 1) near 'elseif'",
        ),
        (b"if x then elseif y", "input:1: 'then' expected near <eof>"),
        (b"if x then elseif", "input:1: unexpected symbol near <eof>"),
        (
            b"if x then else else end",
            "input:1: 'end' expected near 'else'",
        ),
        (
            b"if x\nthen\nelseif y\nthen\n",
            "input:5: 'end' expected (to close 'if' at line 1) near <eof>",
        ),
        (b"for", "input:1: <name> expected near <eof>"),
        (b"for 1", "input:1: <name> expected near '1'"),
        (b"for x", "input:1: '=' or 'in' expected near <eof>"),
        (
            b"for x.y = 1, 2 do end",
            "input:1: '=' or 'in' expected near '.'",
        ),
        (b"for x =", "input:1: unexpected symbol near <eof>"),
        (b"for x = 1", "input:1: ',' expected near <eof>"),
        (b"for x = 1 2", "input:1: ',' expected near '2'"),
        (b"for x = 1,", "input:1: unexpected symbol near <eof>"),
        (b"for x = 1, 2", "input:1: 'do' expected near <eof>"),
        (
            b"for x = 1, 2, 3, 4 do end",
            "input:1: 'do' expected near ','",
        ),
        (b"for x = 1, 2 do", "input:1: 'end' expected near <eof>"),
        (b"for x, = 1", "input:1: <name> expected near '='"),
        (b"for x, y", "input:1: 'in' expected near <eof>"),
        (b"for x, y in", "input:1: unexpected symbol near <eof>"),
        (b"for x in y", "input:1: 'do' expected near <eof>"),
        (
            b"for x in y\n do\n",
            "input:3: 'end' expected (to close 'for' at line 1) near <eof>",
        ),
    ];
    for (source, message) in cases {
        assert_eq!(
            first_error(source),
            message,
            "{}",
            String::from_utf8_lossy(source)
        );
    }
    for source in [&b"end"[..], b"until", b"else", b"x = true end"] {
        let word = String::from_utf8_lossy(source.rsplit(|&byte| byte == b' ').next().unwrap())
            .into_owned();
        assert_eq!(
            first_error(source),
            format!("input:1: <eof> expected near '{word}'")
        );
    }
}

#[test]
fn gives_puc_rios_message_for_each_error_near_a_function_or_a_return() {
    let cases: [(&[u8], &str); 27] = [
        (b"function", "input:1: <name> expected near <eof>"),
        (b"function 1", "input:1: <name> expected near '1'"),
        (b"function f", "input:1: '(' expected near <eof>"),
        (b"function f.", "input:1: <name> expected near <eof>"),
        (b"function f:", "input:1: <name> expected near <eof>"),
        (b"function f:g.h()", "input:1: '(' expected near '.'"),
        (b"function a:b:c() end", "input:1: '(' expected near ':'"),
        (
            b"function f(",
            "input:1: <name> or '...' expected near <eof>",
        ),
        (
            b"function f(1)",
            "input:1: <name> or '...' expected near '1'",
        ),
        (
            b"function f(x,)",
            "input:1: <name> or '...' expected near ')'",
        ),
        (
            b"function f(a,",
            "input:1: <name> or '...' expected near <eof>",
        ),
        (b"function f(...x, y)", "input:1: ')' expected near ','"),
        (b"function f(..., y)", "input:1: ')' expected near ','"),
        (b"function f(a b) end", "input:1: ')' expected near 'b'"),
        (b"function f(...) ", "input:1: 'end' expected near <eof>"),
        (
            b"function f() \n",
            "input:2: 'end' expected (to close 'function' at line 1) near <eof>",
        ),
        (
            b"function f(\n)\n",
            "input:3: 'end' expected (to close 'function' at line 1) near <eof>",
        ),
        (b"function a.b() end x", "input:1: syntax error near <eof>"),
        (
            b"function f() return ... end",
            "input:1: cannot use '...' outside a vararg function near '...'",
        ),
        (
            b"local function f() local g = ... end",
            "input:1: cannot use '...' outside a vararg function near '...'",
        ),
        (b"return return", "input:1: unexpected symbol near 'return'"),
        (b"return 1 2", "input:1: <eof> expected near '2'"),
        (b"return;;", "input:1: <eof> expected near ';'"),
        (b"return 1;x()", "input:1: <eof> expected near 'x'"),
        (b"return 1,", "input:1: unexpected symbol near <eof>"),
        (b"do return 1 2 end", "input:1: 'end' expected near '2'"),
        (
            b"do return\n\n1\n2 end",
            "input:4: 'end' expected (to close 'do' at line 1) near '2'",
        ),
    ];
    for (source, message) in cases {
        assert_eq!(
            first_error(source),
            message,
            "{}",
            String::from_utf8_lossy(source)
        );
    }
    assert_eq!(
        first_error(b"if x then\nreturn\nx = 1\nend"),
        "input:3: 'end' expected (to close 'if' at line 1) near '='"
    );
}

#[test]
fn reports_a_lexical_error_first_when_puc_rio_finds_it_first() {
    let cases: [(&[u8], &str); 14] = [
        (b"x = 1 0x", "input:1: malformed number near '0x'"),
        (b"x 0x", "input:1: malformed number near '0x'"),
        (b"f(1 0x", "input:1: malformed number near '0x'"),
        (b"return 0x", "input:1: malformed number near '0x'"),
        (b"local x <foo> 0x", "input:1: malformed number near '0x'"),
        (
            b"global x <close> 0x",
            "input:1: malformed number near '0x'",
        ),
        (
            b"local x <close>, y <close> 'abc",
            "input:1: unfinished string near <eof>",
        ),
        (
            b"if x then --[[",
            "input:1: unfinished long comment (starting at line 1) near <eof>",
        ),
        (
            b"x = 1\n--[==[\n",
            "input:3: unfinished long comment (starting at line 2) near <eof>",
        ),
        // A syntax error before the token that holds the lexical error comes first.
        (b"@ 0x", "input:1: unexpected symbol near '@'"),
        // The same with no space between the two tokens.
        (b"@\"abc", "input:1: unexpected symbol near '@'"),
        (b"f(1\"abc", "input:1: unfinished string near <eof>"),
        (b"x = = 0x", "input:1: unexpected symbol near '='"),
        (b"local x <foo>\n0x", "input:2: malformed number near '0x'"),
    ];
    for (source, message) in cases {
        assert_eq!(
            first_error(source),
            message,
            "{}",
            String::from_utf8_lossy(source)
        );
    }
    // When the lexical error comes first, the events hold no syntax error.
    let (_, _, errors) = parsed(b"x = 1 0x");
    assert!(errors.is_empty());
}

#[test]
fn gives_each_syntax_error_its_kind_span_line_and_near_text() {
    let span = |start, end| Span {
        source: SourceId::new(0),
        start,
        end,
    };
    let error = |source: &[u8]| {
        let (_, _, errors) = parsed(source);
        let [error] = errors.as_slice() else {
            panic!("one error: {errors:?}");
        };
        error.clone()
    };
    use lunaferra_syntax::Near;
    assert_eq!(
        error(b"x =\n"),
        ParseError {
            kind: ParseErrorKind::UnexpectedSymbol,
            span: span(4, 4),
            line: 2,
            near: Some(Near::Eof),
        }
    );
    assert_eq!(
        error(b"if x then\n"),
        ParseError {
            kind: ParseErrorKind::Unclosed {
                expected: T::End,
                opener: T::If,
                line: 1,
            },
            span: span(10, 10),
            line: 2,
            near: Some(Near::Eof),
        }
    );
    assert_eq!(
        error(b"x = 1 break"),
        ParseError {
            kind: ParseErrorKind::BreakOutsideLoop,
            span: span(6, 11),
            line: 1,
            near: Some(Near::Text(b"break".to_vec())),
        }
    );
    assert_eq!(
        error(b"local x <foo>\n\ny = 1"),
        ParseError {
            kind: ParseErrorKind::UnknownAttribute {
                name: b"foo".to_vec(),
            },
            span: span(9, 12),
            line: 1,
            near: None,
        }
    );
    assert_eq!(
        error(b"local x <close>, y <close>"),
        ParseError {
            kind: ParseErrorKind::MultipleToBeClosed,
            span: span(17, 18),
            line: 1,
            near: None,
        }
    );
    assert_eq!(
        error(b"global x <close>"),
        ParseError {
            kind: ParseErrorKind::GlobalToBeClosed,
            span: span(10, 15),
            line: 1,
            near: None,
        }
    );
    let wide = error(b"x = 1 '\xFF'");
    assert_eq!(wide.message(), b"unexpected symbol near ''\xFF''");
    assert_eq!(wide.to_string(), "unexpected symbol near ''\u{FFFD}''");
    let error: &dyn std::error::Error = &wide;
    assert!(error.source().is_none());
}

#[test]
fn counts_lines_after_a_files_skipped_first_line() {
    let source = b"#!/usr/bin/env lunaferra\nif x";
    let lexed = lex(source, SourceId::new(0), Origin::File).expect("short");
    let errors: Vec<String> = parse(source, &lexed)
        .iter()
        .filter_map(|event| match event {
            Event::Error(error) => Some(format!("{}: {error}", error.line)),
            _ => None,
        })
        .collect();
    assert_eq!(errors, ["2: 'then' expected near <eof>"]);
}

#[test]
fn puts_the_tokens_after_an_error_in_an_error_node_where_it_stopped() {
    let cases = [
        ("x", "(Error (NameExpr x) (Error))"),
        (
            "x, f() = 1",
            "(AssignStat (ExprList (NameExpr x) , (CallExpr (NameExpr f) (Args ( ))) \
             (Error = 1)))",
        ),
        ("if x", "(IfStat if (NameExpr x) (Error))"),
        (
            "if x then @ end y = 1",
            "(IfStat if (NameExpr x) then (Block (Error @ end y = 1)))",
        ),
        ("a.", "(Error (FieldExpr (NameExpr a) . (Error)))"),
        ("for x", "(Error for x (Error))"),
        ("for 1 do end", "(Error for (Error 1 do end))"),
        ("break x", "(Error break x)"),
        (
            "local x <foo> y",
            "(LocalStat local (AttNameList (AttName x (Attrib < foo >) (Error y))))",
        ),
        (
            "x = 1 0x",
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1))) (Error 0x)",
        ),
        (
            "x = 1 --[[",
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1)))",
        ),
        ("0x", "(Error 0x)"),
    ];
    for (source, expected) in cases {
        assert_eq!(block_shape(source), expected, "{source:?}");
    }
}

#[test]
fn reads_a_chain_of_100_000_fields_and_calls_without_recursion() {
    const LINKS: usize = 100_000;
    let mut source = b"x = a".to_vec();
    source.extend(b".b".repeat(LINKS));
    source.extend(b"\nf".iter().chain(b"()".repeat(LINKS).iter()));
    let (tree, _, errors) = parsed(&source);
    assert!(errors.is_empty());
    assert_eq!(tree.root().text(), source);
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let mut statements = chunk.block().expect("a block").statements();
    let Some(Stat::Assign(assign)) = statements.next() else {
        panic!("an assignment first");
    };
    let mut value = assign
        .values()
        .and_then(|values| values.exprs().next())
        .expect("one value");
    let mut fields = 0;
    while let Expr::Field(field) = value {
        fields += 1;
        value = field.object().expect("each field has an object");
    }
    assert_eq!(fields, LINKS);
    assert_eq!(value.syntax().kind(), N::NameExpr);
    let Some(Stat::Call(call)) = statements.next() else {
        panic!("a call second");
    };
    let mut callee = call.call().expect("a call");
    let mut calls = 0;
    while let Expr::Call(call) = callee {
        calls += 1;
        callee = call.callee().expect("each call has a callee");
    }
    assert_eq!(calls, LINKS);
}

#[test]
fn reads_each_statements_parts_through_its_view() {
    let source = "local <const> a, b <close> = 1, 2
global <const> *
global c <const> = 3
global function g(...) return ... end
local function h(p, q, ...rest) end
function m.n.o:p() end
function q() end
nonlocal d, e
x, t.y, t[1] = nil, true, false
o:m'x'
f \"s\"
::top::
goto top
do ; end
while w do break end
repeat until r
if i then elseif j then else end
for k = 0, 10, 2 do end
for k = 0, 10 do end
for k, v in pairs, t do end
return ...
";
    let (tree, _, errors) = parsed(source.as_bytes());
    assert!(errors.is_empty(), "{errors:?}");
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let statements: Vec<Stat<'_>> = chunk.block().expect("a block").statements().collect();
    let [
        Stat::Local(local),
        Stat::Global(star),
        Stat::Global(global),
        Stat::GlobalFunction(global_function),
        Stat::LocalFunction(local_function),
        Stat::Function(method),
        Stat::Function(function),
        Stat::Nonlocal(nonlocal),
        Stat::Assign(assign),
        Stat::Call(method_call),
        Stat::Call(string_call),
        Stat::Label(label),
        Stat::Goto(goto),
        Stat::Do(do_stat),
        Stat::While(while_stat),
        Stat::Repeat(repeat),
        Stat::If(if_stat),
        Stat::NumericFor(stepped),
        Stat::NumericFor(numeric),
        Stat::GenericFor(generic),
        Stat::Return(ret),
    ] = statements.as_slice()
    else {
        panic!("one statement of each kind: {statements:?}");
    };

    check_local(*local);
    assert_eq!(text(star.star()), "*");
    assert_eq!(
        text(star.attrib().and_then(|attrib| attrib.name())),
        "const"
    );
    assert!(star.names().is_none() && star.values().is_none());
    let names = global.names().expect("names");
    assert!(names.attrib().is_none());
    let [name] = names.names().collect::<Vec<_>>()[..] else {
        panic!("one name");
    };
    assert_eq!(text(name.name()), "c");
    assert_eq!(
        text(name.attrib().and_then(|attrib| attrib.name())),
        "const"
    );
    assert_eq!(
        global.values().map(|values| values.exprs().count()),
        Some(1)
    );
    assert!(global.star().is_none() && global.attrib().is_none());

    assert_eq!(text(global_function.name()), "g");
    check_body(global_function.body(), &[], Some(""), 1);
    assert_eq!(text(local_function.name()), "h");
    check_body(local_function.body(), &["p", "q"], Some("rest"), 0);

    let name = method.name().expect("a name");
    assert_eq!(name.syntax().kind(), N::FuncName);
    let path: Vec<String> = name.path().map(|token| text(Some(token))).collect();
    assert_eq!(path, ["m", "n", "o"]);
    assert_eq!(text(name.method()), "p");
    check_body(method.body(), &[], None, 0);
    let name = function.name().expect("a name");
    assert_eq!(name.path().count(), 1);
    assert!(name.method().is_none());

    let names = nonlocal.names().expect("names");
    assert_eq!(names.syntax().kind(), N::NameList);
    let names: Vec<String> = names.names().map(|token| text(Some(token))).collect();
    assert_eq!(names, ["d", "e"]);

    let targets: Vec<String> = assign
        .targets()
        .expect("targets")
        .exprs()
        .map(|expr| expr_text(Some(expr)))
        .collect();
    assert_eq!(targets, ["x", "t.y", "t[1]"]);
    let values = assign.values().expect("values");
    assert_eq!(values.syntax().kind(), N::ExprList);
    let values: Vec<String> = values.exprs().map(|expr| expr_text(Some(expr))).collect();
    assert_eq!(values, ["nil", "true", "false"]);

    assert_eq!(expr_text(method_call.call()), "o:m'x'");
    assert_eq!(expr_text(string_call.call()), "f \"s\"");
    assert_eq!(text(label.name()), "top");
    assert_eq!(text(goto.label()), "top");
    let block = do_stat.block().expect("a block");
    assert!(matches!(block.statements().next(), Some(Stat::Empty(_))));
    assert_eq!(expr_text(while_stat.condition()), "w");
    let body = while_stat.block().expect("a body");
    assert!(matches!(body.statements().next(), Some(Stat::Break(_))));
    assert!(repeat.block().is_some());
    assert_eq!(expr_text(repeat.condition()), "r");
    check_if(*if_stat);
    check_for(*stepped, *numeric, *generic);
    let values = ret.values().expect("values");
    assert!(matches!(values.exprs().next(), Some(Expr::Vararg(_))));
}

/// Checks the parts of `local <const> a, b <close> = 1, 2`.
fn check_local(local: LocalStat<'_>) {
    let names = local.names().expect("names");
    assert_eq!(names.syntax().kind(), N::AttNameList);
    let first = names.attrib().expect("an attribute before the names");
    assert_eq!(first.syntax().kind(), N::Attrib);
    assert_eq!(text(first.name()), "const");
    let [a, b] = names.names().collect::<Vec<_>>()[..] else {
        panic!("two names");
    };
    assert_eq!(a.syntax().kind(), N::AttName);
    assert_eq!(text(a.name()), "a");
    assert!(a.attrib().is_none());
    assert_eq!(text(b.name()), "b");
    assert_eq!(text(b.attrib().and_then(|attrib| attrib.name())), "close");
    let values: Vec<String> = local
        .values()
        .expect("values")
        .exprs()
        .map(|expr| expr_text(Some(expr)))
        .collect();
    assert_eq!(values, ["1", "2"]);
}

/// Checks that `body` has the parameters `params`, a vararg parameter named `vararg` if that is
/// `Some`, and `statements` statements.
fn check_body(
    body: Option<FuncBody<'_>>,
    params: &[&str],
    vararg: Option<&str>,
    statements: usize,
) {
    let body = body.expect("a body");
    assert_eq!(body.syntax().kind(), N::FuncBody);
    let list = body.params().expect("a parameter list");
    assert_eq!(list.syntax().kind(), N::ParamList);
    let names: Vec<String> = list
        .params()
        .map(|param| {
            assert_eq!(param.syntax().kind(), N::Param);
            text(param.name())
        })
        .collect();
    assert_eq!(names, params);
    let found = list.vararg().map(|param| {
        assert_eq!(param.syntax().kind(), N::VarargParam);
        text(param.name())
    });
    assert_eq!(found.as_deref(), vararg);
    let block = body.block().expect("a block");
    assert_eq!(block.statements().count(), statements);
}

/// Checks the parts of `if i then elseif j then else end`.
fn check_if(stat: IfStat<'_>) {
    assert_eq!(expr_text(stat.condition()), "i");
    assert!(stat.block().is_some());
    let [clause] = stat.elseif_clauses().collect::<Vec<_>>()[..] else {
        panic!("one elseif part");
    };
    assert_eq!(clause.syntax().kind(), N::ElseifClause);
    assert_eq!(expr_text(clause.condition()), "j");
    assert!(clause.block().is_some());
    let other = stat.else_clause().expect("an else part");
    assert_eq!(other.syntax().kind(), N::ElseClause);
    assert!(other.block().is_some());
}

/// Checks the parts of `for k = 0, 10, 2 do end`, `for k = 0, 10 do end`, and
/// `for k, v in pairs, t do end`.
fn check_for(
    stepped: NumericForStat<'_>,
    numeric: NumericForStat<'_>,
    generic: GenericForStat<'_>,
) {
    assert_eq!(text(stepped.variable()), "k");
    assert_eq!(expr_text(stepped.start()), "0");
    assert_eq!(expr_text(stepped.stop()), "10");
    assert_eq!(expr_text(stepped.step()), "2");
    assert!(stepped.block().is_some());
    assert!(numeric.step().is_none());
    let names: Vec<String> = generic
        .names()
        .expect("names")
        .names()
        .map(|token| text(Some(token)))
        .collect();
    assert_eq!(names, ["k", "v"]);
    let values: Vec<String> = generic
        .values()
        .expect("values")
        .exprs()
        .map(|expr| expr_text(Some(expr)))
        .collect();
    assert_eq!(values, ["pairs", "t"]);
    assert!(generic.block().is_some());
}

#[test]
fn reads_each_expressions_parts_through_its_view() {
    let source = b"x = a, (b), c.d, e[f], g(h, i), j:k 'l', 2, ...";
    let (tree, _, errors) = parsed(source);
    assert!(errors.is_empty());
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let Some(Stat::Assign(assign)) = chunk.block().expect("a block").statements().next() else {
        panic!("an assignment");
    };
    let values: Vec<Expr<'_>> = assign.values().expect("values").exprs().collect();
    let [
        Expr::Name(name),
        Expr::Paren(paren),
        Expr::Field(field),
        Expr::Index(index),
        Expr::Call(call),
        Expr::MethodCall(method),
        Expr::Literal(literal),
        Expr::Vararg(_),
    ] = values.as_slice()
    else {
        panic!("one expression of each kind: {values:?}");
    };
    assert_eq!(text(name.name()), "a");
    assert_eq!(expr_text(paren.expr()), "b");
    assert_eq!(expr_text(field.object()), "c");
    assert_eq!(text(field.field()), "d");
    assert_eq!(expr_text(index.object()), "e");
    assert_eq!(expr_text(index.index()), "f");
    assert_eq!(expr_text(call.callee()), "g");
    let args = call.args().expect("arguments");
    assert_eq!(args.syntax().kind(), N::Args);
    let args: Vec<String> = args
        .exprs()
        .expect("expressions")
        .exprs()
        .map(|expr| expr_text(Some(expr)))
        .collect();
    assert_eq!(args, ["h", "i"]);
    assert!(call.args().and_then(|args| args.string()).is_none());
    assert_eq!(expr_text(method.object()), "j");
    assert_eq!(text(method.method()), "k");
    let args = method.args().expect("arguments");
    assert_eq!(text(args.string()), "'l'");
    assert!(args.exprs().is_none());
    assert_eq!(literal.token().map(|token| token.kind()), Some(T::Integer));
}

#[test]
fn casts_each_node_to_the_statement_or_expression_of_its_kind() {
    let source = b"local a = 1 global * global function g() end local function h() end \
        function f(...) return ... end nonlocal b x = (y).z[1] o:m() f 's' ::l:: goto l \
        do ; end while w do break end repeat until r if i then end \
        for k = 0, 1 do end for k in t do end return";
    let (tree, _, errors) = parsed(source);
    assert!(errors.is_empty());
    let statements = [
        N::LocalStat,
        N::GlobalStat,
        N::GlobalFunctionStat,
        N::LocalFunctionStat,
        N::FunctionStat,
        N::NonlocalStat,
        N::AssignStat,
        N::CallStat,
        N::LabelStat,
        N::GotoStat,
        N::DoStat,
        N::EmptyStat,
        N::WhileStat,
        N::BreakStat,
        N::RepeatStat,
        N::IfStat,
        N::NumericForStat,
        N::GenericForStat,
        N::ReturnStat,
    ];
    let expressions = [
        N::NameExpr,
        N::ParenExpr,
        N::FieldExpr,
        N::IndexExpr,
        N::CallExpr,
        N::MethodCallExpr,
        N::LiteralExpr,
        N::VarargExpr,
    ];
    let mut seen_statements = Vec::new();
    let mut seen_expressions = Vec::new();
    let mut open = vec![tree.root()];
    while let Some(node) = open.pop() {
        open.extend(node.children().filter_map(SyntaxElement::into_node));
        match Stat::cast(node) {
            Some(stat) => {
                assert_eq!(stat.syntax(), node);
                seen_statements.push(node.kind());
            }
            None => assert!(!statements.contains(&node.kind())),
        }
        match Expr::cast(node) {
            Some(expr) => {
                assert_eq!(expr.syntax(), node);
                seen_expressions.push(node.kind());
            }
            None => assert!(!expressions.contains(&node.kind())),
        }
    }
    for kind in statements {
        assert!(seen_statements.contains(&kind), "{kind:?}");
    }
    for kind in expressions {
        assert!(seen_expressions.contains(&kind), "{kind:?}");
    }
}

#[test]
fn gives_no_part_that_a_broken_statement_lacks() {
    let (tree, _, _) = parsed(b"x, y");
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let Some(Stat::Assign(assign)) = chunk.block().expect("a block").statements().next() else {
        panic!("an assignment");
    };
    assert_eq!(assign.targets().map(|list| list.exprs().count()), Some(2));
    assert!(assign.values().is_none());

    let (tree, _, _) = parsed(b"if x");
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let Some(Stat::If(stat)) = chunk.block().expect("a block").statements().next() else {
        panic!("an if statement");
    };
    assert!(stat.condition().is_some());
    assert!(stat.block().is_none());
    assert!(stat.else_clause().is_none());

    // A statement left without a kind is an error node, not a statement.
    let (tree, _, _) = parsed(b"x");
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    assert_eq!(chunk.block().expect("a block").statements().count(), 0);
}

#[test]
fn gives_back_every_conformance_script_and_its_twin_from_its_parsed_tree() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
    let mut scripts = 0;
    for folder in [root.clone(), root.join("reference")] {
        let mut entries: Vec<_> = std::fs::read_dir(&folder)
            .expect("the conformance folder exists")
            .map(|entry| entry.expect("readable").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "lua"))
            .collect();
        entries.sort();
        for path in entries {
            let source = std::fs::read(&path).expect("readable");
            let lexed = lex(&source, SourceId::new(0), Origin::File).expect("short");
            let events = parse(&source, &lexed);
            let tree = SyntaxTree::build(&source, &lexed, &events).expect("a tree");
            assert_eq!(tree.root().text(), source, "{}", path.display());
            scripts += 1;
        }
    }
    assert!(scripts >= 4);
}

/// Pieces of statements that generated sources are built from: every token that starts or ends
/// a statement, names that are attributes or keywords in some places, and broken tokens.
const PIECES: [&str; 50] = [
    "local", "global", "function", "nonlocal", "return", "break", "goto", "do", "end", "while",
    "repeat", "until", "if", "then", "elseif", "else", "for", "in", "x", "y", "const", "close",
    "=", ",", ".", ":", "::", ";", "(", ")", "[", "]", "<", ">", "*", "...", "1", "'s'", "[[l]]",
    "nil", "+", "{", "f\"{x}\"", "0x", "'open", "--[[", "@", "\0", "\n", "-- c\n",
];

/// Chooses a number below `bound`, as a `usize`.
fn below(source: &mut Source, bound: usize) -> usize {
    let bound = u64::try_from(bound).expect("bounds are small");
    usize::try_from(source.below(bound)).expect("choices are below their bound")
}

/// Generates a source of up to about 30 pieces, with a space or nothing between each two.
fn generate_pieces(source: &mut Source) -> Vec<u8> {
    let mut bytes = Vec::new();
    while source.below(30) != 0 {
        bytes.extend_from_slice(PIECES[below(source, PIECES.len())].as_bytes());
        if source.below(3) != 0 {
            bytes.push(b' ');
        }
    }
    bytes
}

#[test]
fn makes_a_tree_of_every_generated_source_that_gives_it_back() {
    Config::default()
        .with_cases(5_000)
        .assert(generate_pieces, |source| {
            let lexed = lexed(source);
            let events = parse(source, &lexed);
            let Ok(tree) = SyntaxTree::build(source, &lexed, &events) else {
                return false;
            };
            let errors: Vec<&ParseError> = events
                .iter()
                .filter_map(|event| match event {
                    Event::Error(error) => Some(error),
                    _ => None,
                })
                .collect();
            // At most one syntax error, and only before the first lexical error.
            let before_lexical = match (errors.first(), lexed.errors().first()) {
                (Some(error), Some(lexical)) => error.span.end <= lexical.span.start,
                _ => true,
            };
            tree.root().text() == *source && errors.len() <= 1 && before_lexical
        });
}

/// The names that generated programs use. `nonlocal` is a name everywhere but at the start of a
/// statement before a name.
const NAMES: [&str; 4] = ["a", "b", "nonlocal", "c"];

/// How deep a generated program nests, and what its innermost function allows.
#[derive(Clone, Copy)]
struct Scope {
    depth: u32,
    in_loop: bool,
    vararg: bool,
}

/// Generates a valid program of Lua 5.5's statements and the expressions M1.4 reads.
///
/// Each loop goes on, and each optional part is added, only for a choice of 1, so that a choice
/// of 0, which the runner gives once a shrunk case's choices run out, ends the program. An index
/// opens with `[ `, because `[[` always starts a long string.
fn generate_program(source: &mut Source) -> String {
    let mut out = String::new();
    let scope = Scope {
        depth: 3,
        in_loop: false,
        vararg: true,
    };
    generate_block(source, scope, &mut out);
    out
}

fn generate_block(source: &mut Source, scope: Scope, out: &mut String) {
    while source.below(3) != 0 {
        generate_stat(source, scope, out);
        out.push_str(if source.below(4) == 1 { ";\n" } else { "\n" });
    }
    if source.below(4) == 1 {
        out.push_str("return");
        if source.below(2) == 1 {
            out.push(' ');
            generate_exprs(source, scope, out);
        }
        out.push('\n');
    }
}

fn name(source: &mut Source) -> &'static str {
    NAMES[below(source, NAMES.len())]
}

/// Generates a statement. One that starts with `(` gets a `;` before it, because Lua reads a `(`
/// after an expression as a call, so `x = c\n(d).e = 1` would be `x = c(d).e` and a stray `=`.
fn generate_stat(source: &mut Source, scope: Scope, out: &mut String) {
    let mut stat = String::new();
    write_stat(source, scope, &mut stat);
    if stat.starts_with('(') {
        out.push(';');
    }
    out.push_str(&stat);
}

fn write_stat(source: &mut Source, scope: Scope, out: &mut String) {
    let inner = Scope {
        depth: scope.depth.saturating_sub(1),
        ..scope
    };
    let looped = Scope {
        in_loop: true,
        ..inner
    };
    let choices = if scope.depth == 0 { 6 } else { 17 };
    match below(source, choices) {
        0 => {
            generate_target(source, scope, out);
            while source.below(3) == 1 {
                out.push_str(", ");
                generate_target(source, scope, out);
            }
            out.push_str(" = ");
            generate_exprs(source, scope, out);
        }
        1 => generate_call(source, scope, out),
        2 => {
            out.push_str("::");
            out.push_str(name(source));
            out.push_str("::");
        }
        3 => {
            out.push_str("goto ");
            out.push_str(name(source));
        }
        4 => {
            out.push_str(if scope.in_loop { "break" } else { ";" });
        }
        5 => {
            out.push_str("local ");
            generate_attnames(source, out);
            if source.below(2) == 1 {
                out.push_str(" = ");
                generate_exprs(source, scope, out);
            }
        }
        6 => {
            out.push_str("do\n");
            generate_block(source, inner, out);
            out.push_str("end");
        }
        7 => {
            out.push_str("while ");
            generate_expr(source, scope, out);
            out.push_str(" do\n");
            generate_block(source, looped, out);
            out.push_str("end");
        }
        8 => {
            out.push_str("repeat\n");
            generate_block(source, looped, out);
            out.push_str("until ");
            generate_expr(source, scope, out);
        }
        9 => {
            out.push_str("if ");
            generate_expr(source, scope, out);
            out.push_str(" then\n");
            generate_block(source, inner, out);
            while source.below(3) == 1 {
                out.push_str("elseif ");
                generate_expr(source, scope, out);
                out.push_str(" then\n");
                generate_block(source, inner, out);
            }
            if source.below(2) == 1 {
                out.push_str("else\n");
                generate_block(source, inner, out);
            }
            out.push_str("end");
        }
        10 => {
            out.push_str("for ");
            out.push_str(name(source));
            out.push_str(" = ");
            generate_expr(source, scope, out);
            out.push_str(", ");
            generate_expr(source, scope, out);
            if source.below(2) == 1 {
                out.push_str(", ");
                generate_expr(source, scope, out);
            }
            out.push_str(" do\n");
            generate_block(source, looped, out);
            out.push_str("end");
        }
        11 => {
            out.push_str("for ");
            out.push_str(name(source));
            while source.below(2) == 1 {
                out.push_str(", ");
                out.push_str(name(source));
            }
            out.push_str(" in ");
            generate_exprs(source, scope, out);
            out.push_str(" do\n");
            generate_block(source, looped, out);
            out.push_str("end");
        }
        12 => {
            out.push_str("function ");
            out.push_str(name(source));
            while source.below(3) == 1 {
                out.push('.');
                out.push_str(name(source));
            }
            if source.below(3) == 1 {
                out.push(':');
                out.push_str(name(source));
            }
            generate_body(source, inner, out);
        }
        13 => {
            out.push_str(if source.below(2) == 1 {
                "local function "
            } else {
                "global function "
            });
            out.push_str(name(source));
            generate_body(source, inner, out);
        }
        14 => {
            out.push_str("global ");
            if source.below(3) == 1 {
                out.push_str(if source.below(2) == 1 {
                    "*"
                } else {
                    "<const> *"
                });
            } else {
                generate_attnames(source, out);
                if source.below(2) == 1 {
                    out.push_str(" = ");
                    generate_exprs(source, scope, out);
                }
            }
        }
        15 => {
            out.push_str("nonlocal ");
            out.push_str(name(source));
            while source.below(2) == 1 {
                out.push_str(", ");
                out.push_str(name(source));
            }
        }
        _ => out.push(';'),
    }
}

fn generate_attnames(source: &mut Source, out: &mut String) {
    if source.below(3) == 1 {
        out.push_str("<const> ");
    }
    out.push_str(name(source));
    if source.below(3) == 1 {
        out.push_str(" <const>");
    }
    while source.below(2) == 1 {
        out.push_str(", ");
        out.push_str(name(source));
    }
}

fn generate_body(source: &mut Source, scope: Scope, out: &mut String) {
    out.push('(');
    let mut vararg = false;
    if source.below(2) == 1 {
        out.push_str(name(source));
        while source.below(2) == 1 {
            out.push_str(", ");
            out.push_str(name(source));
        }
        if source.below(3) == 1 {
            out.push_str(", ...");
            vararg = true;
        }
    } else if source.below(2) == 1 {
        out.push_str("...");
        if source.below(2) == 1 {
            out.push_str(name(source));
        }
        vararg = true;
    }
    out.push_str(")\n");
    let body = Scope {
        in_loop: false,
        vararg,
        ..scope
    };
    generate_block(source, body, out);
    out.push_str("end");
}

fn generate_exprs(source: &mut Source, scope: Scope, out: &mut String) {
    generate_expr(source, scope, out);
    while source.below(3) == 1 {
        out.push_str(", ");
        generate_expr(source, scope, out);
    }
}

fn generate_expr(source: &mut Source, scope: Scope, out: &mut String) {
    match below(source, 8) {
        0 => out.push_str("nil"),
        1 => out.push_str(if source.below(2) == 1 {
            "true"
        } else {
            "false"
        }),
        2 => out.push_str(if source.below(2) == 1 { "12" } else { "0x1p4" }),
        3 => out.push_str(if source.below(2) == 1 {
            "'s'"
        } else {
            "[==[\nlong]==]"
        }),
        4 if scope.vararg => out.push_str("..."),
        _ => generate_prefix(source, scope, out),
    }
}

/// Generates a name or a parenthesized expression, then fields, indexes, and calls.
fn generate_prefix(source: &mut Source, scope: Scope, out: &mut String) {
    let inner = Scope {
        depth: scope.depth.saturating_sub(1),
        ..scope
    };
    if scope.depth > 0 && source.below(4) == 1 {
        out.push('(');
        generate_expr(source, inner, out);
        out.push(')');
    } else {
        out.push_str(name(source));
    }
    while source.below(3) == 1 {
        generate_suffix(source, inner, out);
    }
}

fn generate_suffix(source: &mut Source, scope: Scope, out: &mut String) {
    match below(source, 4) {
        0 => {
            out.push('.');
            out.push_str(name(source));
        }
        1 if scope.depth > 0 => {
            out.push_str("[ ");
            generate_expr(source, scope, out);
            out.push(']');
        }
        2 => {
            out.push(':');
            out.push_str(name(source));
            generate_args(source, scope, out);
        }
        _ => generate_args(source, scope, out),
    }
}

fn generate_args(source: &mut Source, scope: Scope, out: &mut String) {
    if source.below(3) == 1 {
        out.push_str(" 'arg'");
    } else {
        out.push('(');
        if scope.depth > 0 && source.below(2) == 1 {
            generate_exprs(source, scope, out);
        }
        out.push(')');
    }
}

/// Generates an assignment target: a name, or an expression ending in a field or an index.
fn generate_target(source: &mut Source, scope: Scope, out: &mut String) {
    if source.below(2) == 1 {
        out.push_str(name(source));
        return;
    }
    generate_prefix(source, scope, out);
    if source.below(2) == 1 {
        out.push('.');
        out.push_str(name(source));
    } else {
        out.push_str("[ ");
        generate_expr(source, scope, out);
        out.push(']');
    }
}

/// Generates a call: an expression ending in arguments.
fn generate_call(source: &mut Source, scope: Scope, out: &mut String) {
    generate_prefix(source, scope, out);
    if source.below(2) == 1 {
        out.push(':');
        out.push_str(name(source));
    }
    generate_args(source, scope, out);
}

#[test]
fn accepts_every_generated_program_of_the_statements_it_reads() {
    Config::default()
        .with_cases(2_000)
        .assert(generate_program, |program| {
            first_error(program.as_bytes()).is_empty()
        });
}

#[test]
fn builds_no_tree_from_an_error_event_outside_the_root() {
    let lexed = lexed(b"");
    let [eof] = lexed.tokens() else {
        panic!("an empty source has only its end");
    };
    let (_, _, errors) = parsed(b"x");
    let error = errors.first().expect("an error").clone();
    let events = [
        Event::Error(error),
        Event::Start(N::Chunk),
        Event::Token(eof.kind, eof.span),
        Event::Finish,
    ];
    assert_eq!(
        SyntaxTree::build(b"", &lexed, &events),
        Err(Error::EventOutsideRoot { event: 0 })
    );
}
