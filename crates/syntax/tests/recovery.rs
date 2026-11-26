//! Tests of the parser's recovery after a syntax error, through the public API: that it reports
//! each separate error and keeps the statements between them, where it goes on after each kind
//! of mistake, that it reports no error twice at one token or at a token that holds a lexical
//! error, and that the first error of each chunk in `tests/conformance/syntax_errors.lua` is the
//! one PUC-Rio Lua 5.5.1 printed for the chunk's twin.

use lunaferra_syntax::ast::{AstNode, Chunk, Stat};
use lunaferra_syntax::{
    Code, Lexed, NodeKind, Origin, SourceId, SyntaxElement, SyntaxNode, SyntaxTree, TokenKind,
    Tool, diagnostics, lex, parse, source_errors,
};

/// Lexes `source` as a string, as `load` reads one.
fn lexed(source: &[u8]) -> Lexed {
    lex(source, SourceId::new(0), Origin::String).expect("each test's source is short")
}

/// Parses `source`, and returns its tree, which the parser's events must always make, and each
/// of its errors as `load` writes one for the chunk name `=input`, in the order of
/// `source_errors`.
fn parsed(source: &[u8]) -> (SyntaxTree, Vec<String>) {
    let lexed = lexed(source);
    let events = parse(source, &lexed);
    let tree = SyntaxTree::build(source, &lexed, &events).expect("the events make a tree");
    let errors = source_errors(&lexed, &events)
        .iter()
        .map(|error| format!("input:{}: {error}", error.line()))
        .collect();
    (tree, errors)
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
            SyntaxElement::Token(token) if token.kind() == TokenKind::Eof => out.push_str("<eof>"),
            SyntaxElement::Token(token) => out.push_str(&String::from_utf8_lossy(token.text())),
        }
    }
    out.push(')');
}

/// Returns the shape of each child of the tree's block, separated by spaces.
fn block_shape(tree: &SyntaxTree) -> String {
    let block = tree
        .root()
        .children()
        .find_map(SyntaxElement::into_node)
        .expect("the chunk holds a block");
    let shapes: Vec<String> = block
        .children()
        .filter_map(SyntaxElement::into_node)
        .map(|node| {
            let mut out = String::new();
            write_shape(node, &mut out);
            out
        })
        .collect();
    shapes.join(" ")
}

/// Checks each `(source, errors, shape)` case: the source's errors, in order, and the shape of
/// its block.
fn check(cases: &[(&str, &[&str], &str)]) {
    for &(source, errors, shape) in cases {
        let (tree, found) = parsed(source.as_bytes());
        assert_eq!(found, errors, "{source:?}");
        assert_eq!(block_shape(&tree), shape, "{source:?}");
    }
}

#[test]
fn reports_three_separate_errors_and_keeps_the_statements_between_them() {
    let source = b"local a = = 1\n\
        print(a)\n\
        local b <foo> = 2\n\
        if b then print(b) end\n\
        return a +\n";
    let lexed = lexed(source);
    let events = parse(source, &lexed);
    let found = diagnostics(&lexed, &events);
    let codes: Vec<Code> = found.iter().map(|found| found.code).collect();
    let p = |number| Code::new(Tool::Parser, number);
    assert_eq!(codes, [p(15), p(22), p(15)]);
    let messages: Vec<&str> = found.iter().map(|found| found.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "unexpected symbol near '='",
            "unknown attribute 'foo'",
            "unexpected symbol near <eof>",
        ]
    );

    let tree = SyntaxTree::build(source, &lexed, &events).expect("the events make a tree");
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let statements: Vec<NodeKind> = chunk
        .block()
        .expect("a block")
        .statements()
        .map(|stat| stat.syntax().kind())
        .collect();
    assert_eq!(
        statements,
        [
            NodeKind::LocalStat,
            NodeKind::CallStat,
            NodeKind::LocalStat,
            NodeKind::IfStat,
            NodeKind::ReturnStat,
        ]
    );
    let Some(Stat::If(stat)) = chunk.block().and_then(|block| block.statements().nth(3)) else {
        panic!("the fourth statement is an `if`");
    };
    assert_eq!(
        stat.block().map(|block| block.statements().count()),
        Some(1)
    );
}

#[test]
fn goes_on_after_the_token_that_ends_the_part_where_the_error_is() {
    check(&[
        (
            "if x + then y() end",
            &["input:1: unexpected symbol near 'then'"],
            "(IfStat if (BinaryExpr (NameExpr x) + (Error)) then (Block (CallStat (CallExpr \
             (NameExpr y) (Args ( ))))) end)",
        ),
        (
            "while x + do y() end",
            &["input:1: unexpected symbol near 'do'"],
            "(WhileStat while (BinaryExpr (NameExpr x) + (Error)) do (Block (CallStat (CallExpr \
             (NameExpr y) (Args ( ))))) end)",
        ),
        (
            "repeat x = = 1 until y",
            &["input:1: unexpected symbol near '='"],
            "(RepeatStat repeat (Block (AssignStat (ExprList (NameExpr x)) = (ExprList \
             (Error = 1)))) until (NameExpr y))",
        ),
        (
            "x = a if b + else c",
            &["input:1: unexpected symbol near 'else'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (ConditionalExpr (NameExpr a) if \
             (BinaryExpr (NameExpr b) + (Error)) else (NameExpr c))))",
        ),
        (
            "if a then b = = 1 elseif c then d() else e = = 2 end f()",
            &[
                "input:1: unexpected symbol near '='",
                "input:1: unexpected symbol near '='",
            ],
            "(IfStat if (NameExpr a) then (Block (AssignStat (ExprList (NameExpr b)) = (ExprList \
             (Error = 1)))) (ElseifClause elseif (NameExpr c) then (Block (CallStat (CallExpr \
             (NameExpr d) (Args ( )))))) (ElseClause else (Block (AssignStat (ExprList (NameExpr \
             e)) = (ExprList (Error = 2))))) end) (CallStat (CallExpr (NameExpr f) (Args ( ))))",
        ),
        (
            "do return 1 x = 2 end",
            &["input:1: 'end' expected near 'x'"],
            "(DoStat do (Block (ReturnStat return (ExprList (LiteralExpr 1)))) (Error x = 2) end)",
        ),
        (
            "while x y = 1 end",
            &["input:1: 'do' expected near 'y'"],
            "(WhileStat while (NameExpr x) (Error y = 1) end)",
        ),
        // The token where recovery stops is the one the parser asked for.
        (
            "if x y then z() end",
            &["input:1: 'then' expected near 'y'"],
            "(IfStat if (NameExpr x) (Error y) then (Block (CallStat (CallExpr (NameExpr z) \
             (Args ( ))))) end)",
        ),
        (
            "while x y do z() end",
            &["input:1: 'do' expected near 'y'"],
            "(WhileStat while (NameExpr x) (Error y) do (Block (CallStat (CallExpr (NameExpr z) \
             (Args ( ))))) end)",
        ),
        (
            "if x @ end",
            &["input:1: 'then' expected near '@'"],
            "(IfStat if (NameExpr x) (Error @) end)",
        ),
    ]);
}

#[test]
fn goes_on_at_the_next_statement_after_an_error() {
    check(&[
        (
            "x = = 1; y = 2",
            &["input:1: unexpected symbol near '='"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (Error = 1))) (EmptyStat ;) \
             (AssignStat (ExprList (NameExpr y)) = (ExprList (LiteralExpr 2)))",
        ),
        (
            "local x = = 1 local y = 2",
            &["input:1: unexpected symbol near '='"],
            "(LocalStat local (AttNameList (AttName x)) = (ExprList (Error = 1))) (LocalStat \
             local (AttNameList (AttName y)) = (ExprList (LiteralExpr 2)))",
        ),
        (
            "x = = 1\ny = 2",
            &["input:1: unexpected symbol near '='"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (Error = 1))) (AssignStat \
             (ExprList (NameExpr y)) = (ExprList (LiteralExpr 2)))",
        ),
        // A name, `function`, or `if` stops the skipping only at the start of a line.
        (
            "x = = y z\nfunction f() end",
            &["input:1: unexpected symbol near '='"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (Error = y z))) (FunctionStat \
             function (FuncName f) (FuncBody (ParamList ( )) (Block) end))",
        ),
        (
            "x = = a if b\nif y then end",
            &["input:1: unexpected symbol near '='"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (Error = a if b))) (IfStat if \
             (NameExpr y) then (Block) end)",
        ),
        (
            "for 1 do end",
            &["input:1: <name> expected near '1'"],
            "(Error for (Error 1)) (DoStat do (Block) end)",
        ),
        // In a block, a `,` doesn't stop the skipping; only inside brackets does it.
        (
            "x = = 1, 2\ny = 3",
            &["input:1: unexpected symbol near '='"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (Error = 1 , 2))) (AssignStat \
             (ExprList (NameExpr y)) = (ExprList (LiteralExpr 3)))",
        ),
        (
            "x = 1 end y = 2\nz = 3",
            &["input:1: <eof> expected near 'end'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1))) (Error end y = 2) \
             (AssignStat (ExprList (NameExpr z)) = (ExprList (LiteralExpr 3)))",
        ),
        (
            "return 1 + )\nx = 2",
            &["input:1: unexpected symbol near ')'"],
            "(ReturnStat return (ExprList (BinaryExpr (LiteralExpr 1) + (Error)))) (Error )) \
             (AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 2)))",
        ),
        (
            "f(function()\n  x = = 1\nend)\ny = = 2",
            &[
                "input:2: unexpected symbol near '='",
                "input:4: unexpected symbol near '='",
            ],
            "(CallStat (CallExpr (NameExpr f) (Args ( (ExprList (FunctionExpr function \
             (FuncBody (ParamList ( )) (Block (AssignStat (ExprList (NameExpr x)) = (ExprList \
             (Error = 1)))) end))) )))) (AssignStat (ExprList (NameExpr y)) = (ExprList \
             (Error = 2)))",
        ),
    ]);
}

#[test]
fn goes_on_at_the_bracket_or_the_comma_that_ends_the_element_where_the_error_is() {
    check(&[
        (
            "f(1 2)",
            &["input:1: ')' expected near '2'"],
            "(CallStat (CallExpr (NameExpr f) (Args ( (ExprList (LiteralExpr 1)) (Error 2) ))))",
        ),
        (
            "x = (1 + )\ny = 2",
            &["input:1: unexpected symbol near ')'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (ParenExpr ( (BinaryExpr \
             (LiteralExpr 1) + (Error)) )))) (AssignStat (ExprList (NameExpr y)) = (ExprList \
             (LiteralExpr 2)))",
        ),
        (
            "x = [1, 2 +]",
            &["input:1: unexpected symbol near ']'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (ListExpr [ (LiteralExpr 1) , \
             (BinaryExpr (LiteralExpr 2) + (Error)) ])))",
        ),
        (
            "x = f\"{x +}\" + 1",
            &["input:1: unexpected symbol near '}'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (BinaryExpr (FStringExpr f\" \
             (FStringField { (BinaryExpr (NameExpr x) + (Error)) }) \") + (LiteralExpr 1))))",
        ),
        (
            "f(a +, b)",
            &["input:1: unexpected symbol near ','"],
            "(CallStat (CallExpr (NameExpr f) (Args ( (ExprList (BinaryExpr (NameExpr a) + \
             (Error)) , (NameExpr b)) ))))",
        ),
        (
            "t = {\n  a = 1,\n  b = = 2,\n  c = 3,\n}",
            &["input:3: unexpected symbol near '='"],
            "(AssignStat (ExprList (NameExpr t)) = (ExprList (TableExpr { (NamedField a = \
             (LiteralExpr 1)) , (NamedField b = (Error = 2)) , (NamedField c = (LiteralExpr 3)) \
             , })))",
        ),
        // Inside brackets, a name at the start of a line may start the next element, so it
        // doesn't stop the skipping.
        (
            "t = {\n  a = = 1\n  b = 2\n}",
            &["input:2: unexpected symbol near '='"],
            "(AssignStat (ExprList (NameExpr t)) = (ExprList (TableExpr { (NamedField a = \
             (Error = 1 b = 2)) })))",
        ),
        // A closing bracket whose opening one was skipped is skipped too.
        (
            "x = = f(function() local y = 1 end)\nz = 3",
            &["input:1: unexpected symbol near '='"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (Error = f ( function ( ) local y \
             = 1 end )))) (AssignStat (ExprList (NameExpr z)) = (ExprList (LiteralExpr 3)))",
        ),
        // A missing closing bracket before the next line's statement stops at the statement.
        (
            "print(a\nlocal b = 2",
            &["input:2: ')' expected (to close '(' at line 1) near 'local'"],
            "(CallStat (CallExpr (NameExpr print) (Args ( (ExprList (NameExpr a)) (Error)))) \
             (LocalStat local (AttNameList (AttName b)) = (ExprList (LiteralExpr 2)))",
        ),
    ]);
}

#[test]
fn reports_no_error_twice_at_one_token() {
    check(&[
        (
            "while x do until",
            &["input:1: 'end' expected near 'until'"],
            "(WhileStat while (NameExpr x) do (Block) (Error)) (Error until)",
        ),
        (
            "x = )",
            &["input:1: unexpected symbol near ')'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (Error))) (Error ))",
        ),
        (
            "))\nx = 1",
            &["input:1: unexpected symbol near ')'"],
            "(Error )) (Error )) (AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr \
             1)))",
        ),
        (
            "x = break",
            &["input:1: unexpected symbol near 'break'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (Error))) (BreakStat break)",
        ),
        (
            "x = f if @",
            &["input:1: unexpected symbol near '@'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (NameExpr f))) (IfStat if \
             (Error @))",
        ),
    ]);
}

#[test]
fn reports_no_syntax_error_at_a_token_that_holds_a_lexical_error() {
    check(&[
        (
            "local 0x = 1",
            &["input:1: malformed number near '0x'"],
            "(LocalStat local (AttNameList (AttName (Error 0x = 1))))",
        ),
        // A token the lexer couldn't read stands where it is as an operand.
        (
            "x = 0x + 1",
            &["input:1: malformed number near '0x'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (BinaryExpr (Error 0x) + \
             (LiteralExpr 1))))",
        ),
        (
            "print(\"bad \\q\") y = 1",
            &["input:1: invalid escape sequence near '\"bad \\q'"],
            "(CallStat (CallExpr (NameExpr print) (Args ( (ExprList (Error \"bad \\q\")) )))) \
             (AssignStat (ExprList (NameExpr y)) = (ExprList (LiteralExpr 1)))",
        ),
        (
            "x = f\"\\q{y}\" + 1",
            &["input:1: invalid escape sequence near 'f\"\\q'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (BinaryExpr (FStringExpr f\" \\q \
             (FStringField { (NameExpr y) }) \") + (LiteralExpr 1))))",
        ),
        // What an unfinished string or comment at the end swallowed isn't missing.
        (
            "if x then y = [[abc",
            &["input:1: unfinished long string (starting at line 1) near <eof>"],
            "(IfStat if (NameExpr x) then (Block (AssignStat (ExprList (NameExpr y)) = (ExprList \
             (Error [[abc)))))",
        ),
        (
            "if x then --[[",
            &["input:1: unfinished long comment (starting at line 1) near <eof>"],
            "(IfStat if (NameExpr x) then (Block))",
        ),
        (
            "x = f\"a{1 +",
            &["input:1: unfinished string near <eof>"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (FStringExpr f\" a (FStringField { \
             (BinaryExpr (LiteralExpr 1) +)))))",
        ),
        (
            "if x then y = \"abc",
            &["input:1: unfinished string near <eof>"],
            "(IfStat if (NameExpr x) then (Block (AssignStat (ExprList (NameExpr y)) = (ExprList \
             (Error \"abc)))))",
        ),
        // A malformed number swallows nothing, so the missing `end` is still an error.
        (
            "if x then y = 0x",
            &[
                "input:1: malformed number near '0x'",
                "input:1: 'end' expected near <eof>",
            ],
            "(IfStat if (NameExpr x) then (Block (AssignStat (ExprList (NameExpr y)) = (ExprList \
             (Error 0x)))) (Error))",
        ),
    ]);
}

#[test]
fn reports_a_rule_a_statement_breaks_and_skips_nothing() {
    check(&[
        (
            "x = 1 break y = 2",
            &["input:1: break outside loop near 'break'"],
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1))) (BreakStat \
             break) (AssignStat (ExprList (NameExpr y)) = (ExprList (LiteralExpr 2)))",
        ),
        (
            "function f() return ... end g()",
            &["input:1: cannot use '...' outside a vararg function near '...'"],
            "(FunctionStat function (FuncName f) (FuncBody (ParamList ( )) (Block (ReturnStat \
             return (ExprList (VarargExpr ...)))) end)) (CallStat (CallExpr (NameExpr g) (Args \
             ( ))))",
        ),
        (
            "local a <close>, b <close> = 1, 2\nz = = 3",
            &[
                "input:1: multiple to-be-closed variables in local list",
                "input:2: unexpected symbol near '='",
            ],
            "(LocalStat local (AttNameList (AttName a (Attrib < close >)) , (AttName b (Attrib \
             < close >))) = (ExprList (LiteralExpr 1) , (LiteralExpr 2))) (AssignStat (ExprList \
             (NameExpr z)) = (ExprList (Error = 3)))",
        ),
        (
            "global x <close> = 1",
            &["input:1: global variables cannot be to-be-closed"],
            "(GlobalStat global (AttNameList (AttName x (Attrib < close >))) = (ExprList \
             (LiteralExpr 1)))",
        ),
    ]);
}

#[test]
fn recovers_from_100_000_errors_one_by_one() {
    const LINES: usize = 100_000;
    let super_lexed = lexed;
    let source = "x = = 1\n".repeat(LINES);
    let lexed = super_lexed(source.as_bytes());
    let events = parse(source.as_bytes(), &lexed);
    let tree = SyntaxTree::build(source.as_bytes(), &lexed, &events).expect("a tree");
    assert_eq!(tree.root().text(), source.as_bytes());
    let found = diagnostics(&lexed, &events);
    assert_eq!(found.len(), LINES);
    assert!(
        found
            .iter()
            .all(|found| found.code == Code::new(Tool::Parser, 15))
    );

    // A run of tokens that start no statement is one error.
    let closing = ")".repeat(LINES);
    let closing_lexed = super_lexed(closing.as_bytes());
    let events = parse(closing.as_bytes(), &closing_lexed);
    let tree = SyntaxTree::build(closing.as_bytes(), &closing_lexed, &events).expect("a tree");
    assert_eq!(tree.root().text(), closing.as_bytes());
    assert_eq!(diagnostics(&closing_lexed, &events).len(), 1);
}

/// Returns the text of each chunk that `tests/conformance/syntax_errors.lua` loads: the long
/// string after each name `check`.
fn conformance_chunks(script: &[u8]) -> Vec<Vec<u8>> {
    let lexed = lex(script, SourceId::new(0), Origin::File).expect("the script is short");
    let text = |index: usize| {
        let span = lexed.tokens()[index].span;
        let start = usize::try_from(span.start).expect("small");
        let end = usize::try_from(span.end).expect("small");
        &script[start..end]
    };
    let mut chunks = Vec::new();
    for (index, pair) in lexed.tokens().windows(2).enumerate() {
        // The function's own definition has a `(` after its name.
        let call = pair[0].kind == TokenKind::Name && pair[1].kind == TokenKind::String;
        if !call || text(index) != b"check" {
            continue;
        }
        let string = text(index + 1);
        let level = string
            .iter()
            .skip(1)
            .take_while(|&&byte| byte == b'=')
            .count();
        let bracket = level + 2;
        assert_eq!(string[bracket - 1], b'[', "each chunk is a long string");
        let chunk = &string[bracket..string.len() - bracket];
        assert!(
            !chunk.starts_with(b"\n"),
            "no line break follows the bracket"
        );
        chunks.push(chunk.to_vec());
    }
    chunks
}

#[test]
fn gives_puc_rios_first_error_for_each_chunk_of_the_syntax_errors_conformance_script() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
    let script = std::fs::read(folder.join("syntax_errors.lua")).expect("the script exists");
    let recorded = std::fs::read_to_string(folder.join("reference/syntax_errors.out"))
        .expect("the twin's output is recorded");
    let chunks = conformance_chunks(&script);
    let expected: Vec<&str> = recorded.lines().collect();
    assert_eq!(chunks.len(), expected.len());
    assert!(chunks.len() >= 40);
    for (chunk, expected) in chunks.iter().zip(expected) {
        let (_, errors) = parsed(chunk);
        assert_eq!(
            errors.first().map(String::as_str),
            Some(expected),
            "{}",
            String::from_utf8_lossy(chunk)
        );
    }
}
