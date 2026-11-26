//! Tests of the parser's expressions through the public API: the node kind of each expression,
//! how operators group, the `if` that may start a conditional expression or an `if` statement,
//! the message and line of each syntax error near an expression, the typed views of what it
//! builds, and long chains of operators and elements.
//!
//! Where an expression is Lua 5.5's, each expected message is what `load(source, "=input")` gave
//! for the same source on PUC-Rio Lua 5.5.1 on 2026-11-26. The other messages are Lunaferra's
//! own, which `docs/specs/language.md` records.

use lunaferra_syntax::ast::{AstNode, Chunk, CompareOp, Expr, FStringPart, Stat, TableField};
use lunaferra_syntax::{
    Event, Lexed, NodeKind, Origin, ParseError, ParseErrorKind, SourceId, Span, SyntaxElement,
    SyntaxNode, SyntaxToken, SyntaxTree, TokenKind, lex, parse, source_errors,
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

/// Returns the source's first error, the one PUC-Rio reports, as its `load` writes it for the
/// chunk name `=input`: `input:`, the line, `: `, and the message; or nothing, if the source has
/// no error.
fn first_error(source: &[u8]) -> String {
    let lexed = lexed(source);
    let events = parse(source, &lexed);
    source_errors(&lexed, &events)
        .first()
        .map(|error| format!("input:{}: {error}", error.line()))
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

/// Returns the shape of each child of the chunk's block, separated by spaces, for a source that
/// must parse without an error.
fn shape(source: &str) -> String {
    assert_eq!(first_error(source.as_bytes()), "", "{source:?}");
    let (tree, _, _) = parsed(source.as_bytes());
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

/// Returns the shape of the one value of `x = <value>`, for a source that must be one such
/// assignment.
fn value_shape(value: &str) -> String {
    let source = format!("x = {value}");
    let statement = shape(&source);
    statement
        .strip_prefix("(AssignStat (ExprList (NameExpr x)) = (ExprList ")
        .and_then(|rest| rest.strip_suffix("))"))
        .unwrap_or_else(|| panic!("{source:?} is one assignment: {statement}"))
        .to_owned()
}

/// Returns the one value of the assignment that is the first statement of `tree`.
fn first_value(tree: &SyntaxTree) -> Expr<'_> {
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let Some(Stat::Assign(assign)) = chunk.block().expect("a block").statements().next() else {
        panic!("an assignment first");
    };
    assign
        .values()
        .and_then(|values| values.exprs().next())
        .expect("one value")
}

/// Writes how `node` groups: each binary, unary, comparison, and conditional node in
/// parentheses, with its children separated by spaces, a parenthesized expression as what it
/// holds, and any other node as its text.
fn group(node: SyntaxNode<'_>) -> String {
    match node.kind() {
        N::BinaryExpr | N::UnaryExpr | N::CompareExpr | N::ConditionalExpr => {
            let parts: Vec<String> = node
                .children()
                .map(|child| match child {
                    SyntaxElement::Node(node) => group(node),
                    SyntaxElement::Token(token) => {
                        String::from_utf8_lossy(token.text()).into_owned()
                    }
                })
                .collect();
            format!("({})", parts.join(" "))
        }
        N::ParenExpr => node
            .children()
            .find_map(SyntaxElement::into_node)
            .map_or_else(String::new, group),
        _ => String::from_utf8_lossy(&node.text()).trim().to_owned(),
    }
}

/// Returns how the one value of `x = <value>` groups.
fn grouping(value: &str) -> String {
    let source = format!("x = {value}");
    assert_eq!(first_error(source.as_bytes()), "", "{source:?}");
    let (tree, _, _) = parsed(source.as_bytes());
    group(first_value(&tree).syntax())
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
fn groups_the_examples_as_the_precedence_table_says() {
    let cases = [
        ("2 ** -3 ** 2", "(2 ** (- (3 ** 2)))"),
        ("-x ** 2", "(- (x ** 2))"),
        ("not a == b", "(not (a == b))"),
        ("a < b < c", "(a < b < c)"),
        ("1 + 2 << 3", "((1 + 2) << 3)"),
        ("x if a else y if b else z", "(x if a else (y if b else z))"),
    ];
    for (value, expected) in cases {
        assert_eq!(grouping(value), expected, "{value:?}");
    }
}

#[test]
fn groups_each_level_against_its_neighbors() {
    let cases = [
        ("a or b and c", "(a or (b and c))"),
        ("a and b or c", "((a and b) or c)"),
        ("a or b or c", "((a or b) or c)"),
        ("not a and b", "((not a) and b)"),
        ("not not a", "(not (not a))"),
        ("not a in b", "(not (a in b))"),
        ("a | b < c & d", "((a | b) < (c & d))"),
        ("a | b ^ c & d", "(a | (b ^ (c & d)))"),
        ("a & b ^ c | d", "(((a & b) ^ c) | d)"),
        ("a << b + c", "(a << (b + c))"),
        ("a >> b << c", "((a >> b) << c)"),
        ("a + b * c", "(a + (b * c))"),
        ("a - b - c", "((a - b) - c)"),
        ("a + b - c", "((a + b) - c)"),
        ("a // b % c", "((a // b) % c)"),
        ("a * b / c", "((a * b) / c)"),
        ("-a * b", "((- a) * b)"),
        ("#t + 1", "((# t) + 1)"),
        ("~a ^ b", "((~ a) ^ b)"),
        ("+a", "(+ a)"),
        ("- -a", "(- (- a))"),
        ("a * -b", "(a * (- b))"),
        ("a ** b ** c", "(a ** (b ** c))"),
        ("-a ** -b", "(- (a ** (- b)))"),
        ("2 ** -1", "(2 ** (- 1))"),
        ("a * b ** c", "(a * (b ** c))"),
        ("-t.x ** 2", "(- (t.x ** 2))"),
        ("#t[1]", "(# t[1])"),
        ("-f(x) ** g()", "(- (f(x) ** g()))"),
        ("a < b == c in d", "(a < b == c in d)"),
        (
            "a in b not in c is d is not e",
            "(a in b not in c is d is not e)",
        ),
        ("a <= b >= c > d != e", "(a <= b >= c > d != e)"),
        ("a < b and c < d", "((a < b) and (c < d))"),
        ("a if b or c else d and e", "(a if (b or c) else (d and e))"),
        ("a or b if c else d", "((a or b) if c else d)"),
        ("not a if b else c", "((not a) if b else c)"),
        ("a if not b else c", "(a if (not b) else c)"),
        ("(a + b) * c", "((a + b) * c)"),
        ("(a if b else c) + d", "((a if b else c) + d)"),
        ("a + (b if c else d)", "(a + (b if c else d))"),
        ("'a' + 'b' + 'c'", "(('a' + 'b') + 'c')"),
        ("[a] + [b]", "([a] + [b])"),
        ("f\"{a}\" + b", "(f\"{a}\" + b)"),
        ("{} == {}", "({} == {})"),
        ("function() end or a", "(function() end or a)"),
    ];
    for (value, expected) in cases {
        assert_eq!(grouping(value), expected, "{value:?}");
    }
}

#[test]
fn parses_each_expression_to_its_node_kind() {
    let cases = [
        ("a + b", "(BinaryExpr (NameExpr a) + (NameExpr b))"),
        ("a and b", "(BinaryExpr (NameExpr a) and (NameExpr b))"),
        ("-a", "(UnaryExpr - (NameExpr a))"),
        ("not a", "(UnaryExpr not (NameExpr a))"),
        ("a < b", "(CompareExpr (NameExpr a) < (NameExpr b))"),
        (
            "a not in b is not c",
            "(CompareExpr (NameExpr a) not in (NameExpr b) is not (NameExpr c))",
        ),
        (
            "a if b else c",
            "(ConditionalExpr (NameExpr a) if (NameExpr b) else (NameExpr c))",
        ),
        (
            "function(a) return a end",
            "(FunctionExpr function (FuncBody (ParamList ( (Param a) )) \
             (Block (ReturnStat return (ExprList (NameExpr a)))) end))",
        ),
        ("{}", "(TableExpr { })"),
        (
            "{1, y = 2; [k] = 3,}",
            "(TableExpr { (PositionalField (LiteralExpr 1)) , (NamedField y = (LiteralExpr 2)) ; \
             (IndexField [ (NameExpr k) ] = (LiteralExpr 3)) , })",
        ),
        (
            "{a, a == b}",
            "(TableExpr { (PositionalField (NameExpr a)) , \
             (PositionalField (CompareExpr (NameExpr a) == (NameExpr b))) })",
        ),
        ("[]", "(ListExpr [ ])"),
        (
            "[1, 2,]",
            "(ListExpr [ (LiteralExpr 1) , (LiteralExpr 2) , ])",
        ),
        ("()", "(TupleExpr ( ))"),
        ("(1,)", "(TupleExpr ( (LiteralExpr 1) , ))"),
        (
            "(1, 2)",
            "(TupleExpr ( (LiteralExpr 1) , (LiteralExpr 2) ))",
        ),
        ("(1)", "(ParenExpr ( (LiteralExpr 1) ))"),
        (
            "f\"a{b}c\"",
            "(FStringExpr f\" a (FStringField { (NameExpr b) }) c \")",
        ),
        ("f''", "(FStringExpr f' ')"),
        (
            "f[[{b}]]",
            "(FStringExpr f[[ (FStringField { (NameExpr b) }) ]])",
        ),
        (
            "f\"{b=!r:>{w}.2}\"",
            "(FStringExpr f\" (FStringField { (NameExpr b) = !r (FormatSpec : > \
             (FStringField { (NameExpr w) }) .2) }) \")",
        ),
        (
            "f\"{f'{b}'}\"",
            "(FStringExpr f\" (FStringField { (FStringExpr f' (FStringField { (NameExpr b) }) ') \
             }) \")",
        ),
        (
            "g{1}",
            "(CallExpr (NameExpr g) (Args (TableExpr { (PositionalField (LiteralExpr 1)) })))",
        ),
        (
            "o:m{}",
            "(MethodCallExpr (NameExpr o) : m (Args (TableExpr { })))",
        ),
    ];
    for (value, expected) in cases {
        assert_eq!(value_shape(value), expected, "{value:?}");
    }
}

#[test]
fn parses_the_literal_examples_of_the_task() {
    assert_eq!(value_shape("(x)"), "(ParenExpr ( (NameExpr x) ))");
    assert_eq!(value_shape("(x,)"), "(TupleExpr ( (NameExpr x) , ))");
    assert_eq!(value_shape("()"), "(TupleExpr ( ))");
    assert_eq!(
        value_shape("[ [1], [] ]"),
        "(ListExpr [ (ListExpr [ (LiteralExpr 1) ]) , (ListExpr [ ]) ])"
    );
    // `:` before a name and `(` that touch it is a method call; any other `:` starts a spec.
    assert_eq!(
        value_shape("f\"{p:length()}\""),
        "(FStringExpr f\" (FStringField { (MethodCallExpr (NameExpr p) : length (Args ( ))) }) \")"
    );
    assert_eq!(
        value_shape("f\"{x:>8}\""),
        "(FStringExpr f\" (FStringField { (NameExpr x) (FormatSpec : >8) }) \")"
    );
}

#[test]
fn reads_a_field_that_starts_with_a_bracket_as_a_key_only_before_an_equal_sign() {
    let cases = [
        (
            "{[k] = v}",
            "(TableExpr { (IndexField [ (NameExpr k) ] = (NameExpr v)) })",
        ),
        (
            "{[ [1] ] = v}",
            "(TableExpr { (IndexField [ (ListExpr [ (LiteralExpr 1) ]) ] = (NameExpr v)) })",
        ),
        (
            "{[1]}",
            "(TableExpr { (PositionalField (ListExpr [ (LiteralExpr 1) ])) })",
        ),
        (
            "{[1, 2], [], [3,]}",
            "(TableExpr { (PositionalField (ListExpr [ (LiteralExpr 1) , (LiteralExpr 2) ])) , \
             (PositionalField (ListExpr [ ])) , \
             (PositionalField (ListExpr [ (LiteralExpr 3) , ])) })",
        ),
        (
            "{[1] + y}",
            "(TableExpr { (PositionalField (BinaryExpr (ListExpr [ (LiteralExpr 1) ]) + \
             (NameExpr y))) })",
        ),
        (
            "{[1] == [1]}",
            "(TableExpr { (PositionalField (CompareExpr (ListExpr [ (LiteralExpr 1) ]) == \
             (ListExpr [ (LiteralExpr 1) ]))) })",
        ),
        (
            "{[a] if c else d}",
            "(TableExpr { (PositionalField (ConditionalExpr (ListExpr [ (NameExpr a) ]) if \
             (NameExpr c) else (NameExpr d))) })",
        ),
        (
            "{[a, b] * 2}",
            "(TableExpr { (PositionalField (BinaryExpr (ListExpr [ (NameExpr a) , (NameExpr b) \
             ]) * (LiteralExpr 2))) })",
        ),
        (
            "{[] + [a, b]}",
            "(TableExpr { (PositionalField (BinaryExpr (ListExpr [ ]) + (ListExpr [ (NameExpr a) \
             , (NameExpr b) ]))) })",
        ),
    ];
    for (value, expected) in cases {
        assert_eq!(value_shape(value), expected, "{value:?}");
    }
}

#[test]
fn takes_no_field_index_or_call_after_a_tuple() {
    assert_eq!(
        first_error(b"x = (1, 2)[0]"),
        "input:1: unexpected symbol near '['"
    );
    assert_eq!(
        first_error(b"x = (1, 2).y"),
        "input:1: unexpected symbol near '.'"
    );
    assert_eq!(first_error(b"(a, b) = 1"), "input:1: syntax error near '='");
    assert_eq!(first_error(b"(f, g)()"), "input:1: syntax error near '('");
    // A parenthesized expression is a prefix expression, as in Lua, even around a tuple.
    assert_eq!(
        value_shape("(t)[0]"),
        "(IndexExpr (ParenExpr ( (NameExpr t) )) [ (LiteralExpr 0) ])"
    );
    assert_eq!(
        value_shape("((1, 2))[0]"),
        "(IndexExpr (ParenExpr ( (TupleExpr ( (LiteralExpr 1) , (LiteralExpr 2) )) )) [ \
         (LiteralExpr 0) ])"
    );
    assert_eq!(
        first_error(b"x = [1, 2][0]"),
        "input:1: unexpected symbol near '['"
    );
}

#[test]
fn reads_an_if_after_a_value_as_an_if_statement_when_no_else_follows_its_condition() {
    assert_eq!(
        shape("x = f if c then g() end"),
        "(AssignStat (ExprList (NameExpr x)) = (ExprList (NameExpr f))) \
         (IfStat if (NameExpr c) then (Block (CallStat (CallExpr (NameExpr g) (Args ( ))))) end)"
    );
    assert_eq!(
        shape("x = f if c else g"),
        "(AssignStat (ExprList (NameExpr x)) = (ExprList (ConditionalExpr (NameExpr f) if \
         (NameExpr c) else (NameExpr g))))"
    );
    let cases = [
        (
            "local x = 1 if c then end",
            "(LocalStat local (AttNameList (AttName x)) = (ExprList (LiteralExpr 1))) \
             (IfStat if (NameExpr c) then (Block) end)",
        ),
        (
            "global x = 1 if c then end",
            "(GlobalStat global (AttNameList (AttName x)) = (ExprList (LiteralExpr 1))) \
             (IfStat if (NameExpr c) then (Block) end)",
        ),
        (
            "repeat until r if c then end",
            "(RepeatStat repeat (Block) until (NameExpr r)) (IfStat if (NameExpr c) then (Block) \
             end)",
        ),
        (
            "x, y = 1, 2 if c then end",
            "(AssignStat (ExprList (NameExpr x) , (NameExpr y)) = (ExprList (LiteralExpr 1) , \
             (LiteralExpr 2))) (IfStat if (NameExpr c) then (Block) end)",
        ),
        // The condition goes on as an `if` statement's would.
        (
            "x = f if a if b else c then end",
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (NameExpr f))) \
             (IfStat if (ConditionalExpr (NameExpr a) if (NameExpr b) else (NameExpr c)) then \
             (Block) end)",
        ),
        (
            "x = f if a or b then elseif d then else end",
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (NameExpr f))) \
             (IfStat if (BinaryExpr (NameExpr a) or (NameExpr b)) then (Block) \
             (ElseifClause elseif (NameExpr d) then (Block)) (ElseClause else (Block)) end)",
        ),
        // The last value of a conditional ends the statement too.
        (
            "x = a if b else c if d then end",
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (ConditionalExpr (NameExpr a) if \
             (NameExpr b) else (NameExpr c)))) (IfStat if (NameExpr d) then (Block) end)",
        ),
        // The `if` statement is a statement of the block, before the next one.
        (
            "do x = 1 if c then end y = 2 end",
            "(DoStat do (Block (AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1))) \
             (IfStat if (NameExpr c) then (Block) end) (AssignStat (ExprList (NameExpr y)) = \
             (ExprList (LiteralExpr 2)))) end)",
        ),
        (
            "x = 1\nif c then end\nreturn",
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (LiteralExpr 1))) \
             (IfStat if (NameExpr c) then (Block) end) (ReturnStat return)",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(shape(source), expected, "{source:?}");
    }
}

#[test]
fn reads_an_if_after_an_expression_that_ends_no_statement_as_a_conditional_expression() {
    let cases: [(&[u8], &str); 10] = [
        (
            b"return f if c then end",
            "input:1: 'else' expected near 'then'",
        ),
        (b"return f if c", "input:1: 'else' expected near <eof>"),
        (b"f(a if b)", "input:1: 'else' expected near ')'"),
        (
            b"if a if b then end",
            "input:1: 'else' expected near 'then'",
        ),
        (b"while a if b do end", "input:1: 'else' expected near 'do'"),
        (b"x = t[a if b]", "input:1: 'else' expected near ']'"),
        (b"x = {a if b}", "input:1: 'else' expected near '}'"),
        (b"x = (a if b)", "input:1: 'else' expected near ')'"),
        (
            b"for i = a if b, c do end",
            "input:1: 'else' expected near ','",
        ),
        (b"x = f\"{a if b}\"", "input:1: 'else' expected near '}'"),
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
        shape("return a if b else c"),
        "(ReturnStat return (ExprList (ConditionalExpr (NameExpr a) if (NameExpr b) else \
         (NameExpr c))))"
    );
}

#[test]
fn gives_puc_rios_message_for_each_error_near_an_if_after_a_value() {
    let cases: [(&[u8], &str); 8] = [
        (b"x = f if c", "input:1: 'then' expected near <eof>"),
        (b"x = f if c, d", "input:1: 'then' expected near ','"),
        (b"x = f if", "input:1: unexpected symbol near <eof>"),
        (b"x = 1 if 2 3", "input:1: 'then' expected near '3'"),
        (
            b"local x = 1 if c end",
            "input:1: 'then' expected near 'end'",
        ),
        (
            b"x = f if c then\n",
            "input:2: 'end' expected (to close 'if' at line 1) near <eof>",
        ),
        (
            b"repeat until r if",
            "input:1: unexpected symbol near <eof>",
        ),
        (
            b"x = 1\nif c then\n\nelse",
            "input:4: 'end' expected (to close 'if' at line 2) near <eof>",
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
fn reports_luas_operators_with_the_ones_to_write_instead() {
    let cases: [(&[u8], &str); 14] = [
        (
            b"x = a .. b",
            "input:1: '..' is not an operator: use '+' to join strings",
        ),
        (
            b"x = a ~= b",
            "input:1: '~=' is not an operator: use '!=' to test inequality",
        ),
        (
            b"x = a ~ b",
            "input:1: '~' is not a binary operator: use '^' for bitwise XOR",
        ),
        (
            b"x = a\n.. b",
            "input:2: '..' is not an operator: use '+' to join strings",
        ),
        (
            b"x = a ~ ~b",
            "input:1: '~' is not a binary operator: use '^' for bitwise XOR",
        ),
        (
            b"x = -a .. b",
            "input:1: '..' is not an operator: use '+' to join strings",
        ),
        (
            b"x = a + b .. c",
            "input:1: '..' is not an operator: use '+' to join strings",
        ),
        (
            b"x = a < b ~= c",
            "input:1: '~=' is not an operator: use '!=' to test inequality",
        ),
        (
            b"x = not a ~= b",
            "input:1: '~=' is not an operator: use '!=' to test inequality",
        ),
        (
            b"f(a .. b)",
            "input:1: '..' is not an operator: use '+' to join strings",
        ),
        (
            b"x = {a ~ b}",
            "input:1: '~' is not a binary operator: use '^' for bitwise XOR",
        ),
        (
            b"x = f\"{a .. b}\"",
            "input:1: '..' is not an operator: use '+' to join strings",
        ),
        (
            b"if a ~= b then end",
            "input:1: '~=' is not an operator: use '!=' to test inequality",
        ),
        (
            b"return a .. b",
            "input:1: '..' is not an operator: use '+' to join strings",
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
    // Before an operand, `~` is bitwise NOT, and `...` is the vararg expression.
    assert_eq!(value_shape("~a"), "(UnaryExpr ~ (NameExpr a))");
    assert_eq!(
        value_shape("a ^ ~b"),
        "(BinaryExpr (NameExpr a) ^ (UnaryExpr ~ (NameExpr b)))"
    );
    assert_eq!(
        first_error(b"x = .. b"),
        "input:1: unexpected symbol near '..'"
    );
    assert_eq!(
        first_error(b"x = ~= b"),
        "input:1: unexpected symbol near '~='"
    );
    // A statement that starts with an expression is read as Lua reads it.
    assert_eq!(first_error(b"a .. b"), "input:1: syntax error near '..'");
    assert_eq!(first_error(b"a + b"), "input:1: syntax error near '+'");
}

#[test]
fn gives_each_lua_operator_error_its_kind_span_and_line_without_near() {
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
    assert_eq!(
        error(b"x = a\n\n.. b"),
        ParseError {
            kind: ParseErrorKind::LuaConcatenation,
            span: span(7, 9),
            line: 3,
            near: None,
        }
    );
    assert_eq!(
        error(b"x = a ~= b"),
        ParseError {
            kind: ParseErrorKind::LuaNotEqual,
            span: span(6, 8),
            line: 1,
            near: None,
        }
    );
    assert_eq!(
        error(b"x = a ~ b"),
        ParseError {
            kind: ParseErrorKind::LuaBitwiseXor,
            span: span(6, 7),
            line: 1,
            near: None,
        }
    );
}

#[test]
fn gives_puc_rios_message_for_each_error_near_an_operator() {
    let cases: [(&[u8], &str); 18] = [
        (b"x = 1 +", "input:1: unexpected symbol near <eof>"),
        (b"x = -", "input:1: unexpected symbol near <eof>"),
        (b"x = #", "input:1: unexpected symbol near <eof>"),
        (b"x = not", "input:1: unexpected symbol near <eof>"),
        (b"x = 1 and", "input:1: unexpected symbol near <eof>"),
        (b"x = 1 or or", "input:1: unexpected symbol near 'or'"),
        (b"x = 1 ==", "input:1: unexpected symbol near <eof>"),
        (b"x = a < b <", "input:1: unexpected symbol near <eof>"),
        (b"x = (1 +)", "input:1: unexpected symbol near ')'"),
        (b"x = 1 *\n\n", "input:3: unexpected symbol near <eof>"),
        (b"x = 1 + * 2", "input:1: unexpected symbol near '*'"),
        (b"x = 1 & | 2", "input:1: unexpected symbol near '|'"),
        (b"x = 1 << >> 2", "input:1: unexpected symbol near '>>'"),
        (b"x = 1 % / 2", "input:1: unexpected symbol near '/'"),
        (b"x = a <= >= b", "input:1: unexpected symbol near '>='"),
        (b"x = - * a", "input:1: unexpected symbol near '*'"),
        (b"x = a and or b", "input:1: unexpected symbol near 'or'"),
        (b"f(1 +)", "input:1: unexpected symbol near ')'"),
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
fn gives_a_message_for_each_error_near_lunaferras_own_operators() {
    let cases: [(&[u8], &str); 16] = [
        (b"x = 2 **", "input:1: unexpected symbol near <eof>"),
        (b"x = 2 ** * 3", "input:1: unexpected symbol near '*'"),
        (b"x = a is", "input:1: unexpected symbol near <eof>"),
        (b"x = a is not", "input:1: unexpected symbol near <eof>"),
        (b"x = a not in", "input:1: unexpected symbol near <eof>"),
        (b"x = a in", "input:1: unexpected symbol near <eof>"),
        (b"x = a != ", "input:1: unexpected symbol near <eof>"),
        // `not` binds more loosely than a comparison or any tighter operator, so it may not
        // start one of their operands, as in Python.
        (b"x = a == not b", "input:1: unexpected symbol near 'not'"),
        (b"x = a + not b", "input:1: unexpected symbol near 'not'"),
        (b"x = - not a", "input:1: unexpected symbol near 'not'"),
        (b"x = 2 ** not a", "input:1: unexpected symbol near 'not'"),
        (
            b"x = a is not not b",
            "input:1: unexpected symbol near 'not'",
        ),
        // `not` before anything but `in` ends the expression, and starts no statement.
        (b"x = a not b", "input:1: unexpected symbol near 'not'"),
        (b"x = a if b else", "input:1: unexpected symbol near <eof>"),
        (b"x = a if", "input:1: unexpected symbol near <eof>"),
        (
            b"x = a if b else c else d",
            "input:1: <eof> expected near 'else'",
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
fn gives_puc_rios_message_for_each_error_near_a_table_or_a_function() {
    let cases: [(&[u8], &str); 22] = [
        (b"x = {", "input:1: unexpected symbol near <eof>"),
        (b"x = {\n", "input:2: unexpected symbol near <eof>"),
        (b"x = {1", "input:1: '}' expected near <eof>"),
        (
            b"x = {\n1",
            "input:2: '}' expected (to close '{' at line 1) near <eof>",
        ),
        (b"x = {1 2}", "input:1: '}' expected near '2'"),
        (b"x = {x = }", "input:1: unexpected symbol near '}'"),
        (b"x = {x =", "input:1: unexpected symbol near <eof>"),
        (b"x = {x = 1 y = 2}", "input:1: '}' expected near 'y'"),
        (b"x = {[1 = 2}", "input:1: ']' expected near '='"),
        (b"x = {[\n1", "input:2: ']' expected near <eof>"),
        (b"x = {[1] = }", "input:1: unexpected symbol near '}'"),
        (b"x = {[", "input:1: unexpected symbol near <eof>"),
        (b"x = {,}", "input:1: unexpected symbol near ','"),
        (b"x = {;}", "input:1: unexpected symbol near ';'"),
        (b"x = {1,,}", "input:1: unexpected symbol near ','"),
        (b"f{", "input:1: unexpected symbol near <eof>"),
        (b"f{1", "input:1: '}' expected near <eof>"),
        (b"x = function", "input:1: '(' expected near <eof>"),
        (
            b"x = function(",
            "input:1: <name> or '...' expected near <eof>",
        ),
        (b"x = function() ", "input:1: 'end' expected near <eof>"),
        (
            b"x = function()\n",
            "input:2: 'end' expected (to close 'function' at line 1) near <eof>",
        ),
        (
            b"x = function() return ... end",
            "input:1: cannot use '...' outside a vararg function near '...'",
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
fn gives_a_message_for_each_error_near_a_bracket_field_a_list_or_a_tuple() {
    let cases: [(&[u8], &str); 17] = [
        // A field that starts with `[` and isn't a key is a list, so these differ from PUC-Rio,
        // which expects `=` after the key.
        (b"x = {[1] 2}", "input:1: '}' expected near '2'"),
        (
            b"x = {\n[1]",
            "input:2: '}' expected (to close '{' at line 1) near <eof>",
        ),
        (b"x = {[1, 2] = 3}", "input:1: '}' expected near '='"),
        (b"x = {[] = 1}", "input:1: '}' expected near '='"),
        (b"x = {[1, 2", "input:1: ']' expected near <eof>"),
        (b"x = {[\n1,", "input:2: unexpected symbol near <eof>"),
        (
            b"x = {[1,\n2",
            "input:2: ']' expected (to close '[' at line 1) near <eof>",
        ),
        (b"x = [", "input:1: unexpected symbol near <eof>"),
        (b"x = [1", "input:1: ']' expected near <eof>"),
        (
            b"x = [\n1",
            "input:2: ']' expected (to close '[' at line 1) near <eof>",
        ),
        (b"x = [1 2]", "input:1: ']' expected near '2'"),
        (b"x = [,]", "input:1: unexpected symbol near ','"),
        (b"x = [1,,]", "input:1: unexpected symbol near ','"),
        (b"x = (1,", "input:1: unexpected symbol near <eof>"),
        (b"x = (1, 2", "input:1: ')' expected near <eof>"),
        (
            b"x = (1,\n2",
            "input:2: ')' expected (to close '(' at line 1) near <eof>",
        ),
        (b"x = (,)", "input:1: unexpected symbol near ','"),
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
fn gives_a_message_for_each_error_in_an_f_strings_field() {
    let cases: [(&[u8], &str); 9] = [
        (b"x = f\"{}\"", "input:1: unexpected symbol near '}'"),
        (b"x = f\"{a b}\"", "input:1: '}' expected near 'b'"),
        (b"x = f\"{a=b}\"", "input:1: '}' expected near 'b'"),
        (b"x = f\"{a = = }\"", "input:1: '}' expected near '='"),
        (b"x = f\"{a!r=}\"", "input:1: '}' expected near '!'"),
        (b"x = f\"{x:>8\"", "input:1: '}' expected near '\"'"),
        (b"x = f[[{x:>8]]", "input:1: '}' expected near ']]'"),
        (b"x = f\"{x:{}}\"", "input:1: unexpected symbol near '}'"),
        (b"x = f\"{\n\na b}\"", "input:3: '}' expected near 'b'"),
    ];
    for (source, message) in cases {
        assert_eq!(
            first_error(source),
            message,
            "{}",
            String::from_utf8_lossy(source)
        );
    }
    // A field may span lines, hold comments, and hold strings and braces of its own.
    assert_eq!(
        value_shape("f\"{ -- the count\n  t[\"k\"] + #{1} }\""),
        "(FStringExpr f\" (FStringField { (BinaryExpr (IndexExpr (NameExpr t) [ \
         (LiteralExpr \"k\") ]) + (UnaryExpr # (TableExpr { (PositionalField (LiteralExpr 1)) \
         }))) }) \")"
    );
}

#[test]
fn reads_each_expressions_parts_through_its_view() {
    let source = b"x = a + b, -c, d < e not in f is not g is h, i if j else k, \
        function(l) end, {m, n = 1, [o] = 2}, [p, q], (r, s), \
        f\"t{u=!r:>{v}}w\", y{}, z < z <= z > z >= z == z != z in z";
    let (tree, _, errors) = parsed(source);
    assert!(errors.is_empty(), "{errors:?}");
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let Some(Stat::Assign(assign)) = chunk.block().expect("a block").statements().next() else {
        panic!("an assignment");
    };
    let values: Vec<Expr<'_>> = assign.values().expect("values").exprs().collect();
    let [
        Expr::Binary(binary),
        Expr::Unary(unary),
        Expr::Compare(compare),
        Expr::Conditional(conditional),
        Expr::Function(function),
        Expr::Table(table),
        Expr::List(list),
        Expr::Tuple(tuple),
        Expr::FString(fstring),
        Expr::Call(call),
        Expr::Compare(every),
    ] = values.as_slice()
    else {
        panic!("one expression of each kind: {values:?}");
    };

    assert_eq!(expr_text(binary.left()), "a");
    assert_eq!(text(binary.operator()), "+");
    assert_eq!(expr_text(binary.right()), "b");
    assert_eq!(text(unary.operator()), "-");
    assert_eq!(expr_text(unary.operand()), "c");

    let operands: Vec<String> = compare
        .operands()
        .map(|expr| expr_text(Some(expr)))
        .collect();
    assert_eq!(operands, ["d", "e", "f", "g", "h"]);
    let operators: Vec<CompareOp> = compare.operators().collect();
    assert_eq!(
        operators,
        [
            CompareOp::Less,
            CompareOp::NotIn,
            CompareOp::IsNot,
            CompareOp::Is
        ]
    );
    let operators: Vec<CompareOp> = every.operators().collect();
    assert_eq!(
        operators,
        [
            CompareOp::Less,
            CompareOp::LessEqual,
            CompareOp::Greater,
            CompareOp::GreaterEqual,
            CompareOp::Equal,
            CompareOp::NotEqual,
            CompareOp::In,
        ]
    );
    assert_eq!(every.operands().count(), 8);

    assert_eq!(expr_text(conditional.then_value()), "i");
    assert_eq!(expr_text(conditional.condition()), "j");
    assert_eq!(expr_text(conditional.else_value()), "k");

    let body = function.body().expect("a body");
    assert_eq!(body.params().map(|params| params.params().count()), Some(1));

    let fields: Vec<TableField<'_>> = table.fields().collect();
    let [
        TableField::Positional(positional),
        TableField::Named(named),
        TableField::Index(index),
    ] = fields.as_slice()
    else {
        panic!("one field of each kind: {fields:?}");
    };
    assert_eq!(expr_text(positional.value()), "m");
    assert_eq!(text(named.name()), "n");
    assert_eq!(expr_text(named.value()), "1");
    assert_eq!(expr_text(index.key()), "o");
    assert_eq!(expr_text(index.value()), "2");
    assert_eq!(positional.syntax().kind(), N::PositionalField);
    assert_eq!(named.syntax().kind(), N::NamedField);
    assert_eq!(index.syntax().kind(), N::IndexField);

    let elements: Vec<String> = list.elements().map(|expr| expr_text(Some(expr))).collect();
    assert_eq!(elements, ["p", "q"]);
    let elements: Vec<String> = tuple.elements().map(|expr| expr_text(Some(expr))).collect();
    assert_eq!(elements, ["r", "s"]);

    let parts: Vec<FStringPart<'_>> = fstring.parts().collect();
    let [
        FStringPart::Text(before),
        FStringPart::Field(field),
        FStringPart::Text(after),
    ] = parts.as_slice()
    else {
        panic!("text, a field, and text: {parts:?}");
    };
    assert_eq!(before.text(), b"t");
    assert_eq!(after.text(), b"w");
    assert_eq!(field.syntax().kind(), N::FStringField);
    assert_eq!(expr_text(field.expr()), "u");
    assert_eq!(text(field.equal()), "=");
    assert_eq!(text(field.conversion()), "!r");
    let spec = field.spec().expect("a spec");
    assert_eq!(spec.syntax().kind(), N::FormatSpec);
    let parts: Vec<FStringPart<'_>> = spec.parts().collect();
    let [FStringPart::Text(align), FStringPart::Field(width)] = parts.as_slice() else {
        panic!("text and a field: {parts:?}");
    };
    assert_eq!(align.text(), b">");
    assert_eq!(expr_text(width.expr()), "v");
    assert!(width.equal().is_none() && width.conversion().is_none() && width.spec().is_none());

    let args = call.args().expect("arguments");
    let table = args.table().expect("a table");
    assert_eq!(table.fields().count(), 0);
    assert!(args.exprs().is_none() && args.string().is_none());
}

#[test]
fn casts_each_new_node_to_the_expression_of_its_kind() {
    let source = b"x = a + b, -c, d < e, f if g else h, function() end, {}, [], (), f\"\"";
    let (tree, _, errors) = parsed(source);
    assert!(errors.is_empty());
    let kinds = [
        N::BinaryExpr,
        N::UnaryExpr,
        N::CompareExpr,
        N::ConditionalExpr,
        N::FunctionExpr,
        N::TableExpr,
        N::ListExpr,
        N::TupleExpr,
        N::FStringExpr,
    ];
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let Some(Stat::Assign(assign)) = chunk.block().expect("a block").statements().next() else {
        panic!("an assignment");
    };
    let values: Vec<Expr<'_>> = assign.values().expect("values").exprs().collect();
    assert_eq!(values.len(), kinds.len());
    for (value, kind) in values.iter().zip(kinds) {
        assert_eq!(value.syntax().kind(), kind);
        assert_eq!(Expr::cast(value.syntax()), Some(*value));
        // No other view takes the node.
        assert!(TableField::cast(value.syntax()).is_none());
    }
}

#[test]
fn gives_no_part_that_a_broken_expression_lacks() {
    let (tree, _, _) = parsed(b"x = a +");
    let Expr::Binary(binary) = first_value(&tree) else {
        panic!("a binary expression");
    };
    assert!(binary.left().is_some());
    assert!(binary.right().is_none());

    let (tree, _, _) = parsed(b"f(a if b)");
    let chunk = Chunk::cast(tree.root()).expect("the root is a chunk");
    let Some(Stat::Call(call)) = chunk.block().expect("a block").statements().next() else {
        panic!("a call");
    };
    let Some(Expr::Call(call)) = call.call() else {
        panic!("a call expression");
    };
    let args = call
        .args()
        .and_then(|args| args.exprs())
        .expect("arguments");
    let Some(Expr::Conditional(conditional)) = args.exprs().next() else {
        panic!("a conditional expression");
    };
    assert_eq!(expr_text(conditional.condition()), "b");
    assert!(conditional.else_value().is_none());

    let (tree, _, _) = parsed(b"x = {[1 = 2}");
    let Expr::Table(table) = first_value(&tree) else {
        panic!("a table");
    };
    let Some(TableField::Index(field)) = table.fields().next() else {
        panic!("a key field");
    };
    assert_eq!(expr_text(field.key()), "1");
    assert!(field.value().is_none());
}

#[test]
fn puts_the_tokens_it_skips_after_an_error_where_the_parser_recovers() {
    let block_shape = |source: &str| {
        let (tree, _, _) = parsed(source.as_bytes());
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
    };
    let cases = [
        (
            "x = a .. b",
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (NameExpr a) (Error .. b)))",
        ),
        (
            "x = f if @",
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (NameExpr f))) \
             (IfStat if (Error @))",
        ),
        (
            "x = {[1 = 2}",
            "(AssignStat (ExprList (NameExpr x)) = (ExprList (TableExpr { (IndexField [ \
             (LiteralExpr 1) (Error = 2)) })))",
        ),
        (
            "f(a if b)",
            "(CallStat (CallExpr (NameExpr f) (Args ( (ExprList (ConditionalExpr (NameExpr a) \
             if (NameExpr b) (Error))) ))))",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(block_shape(source), expected, "{source:?}");
    }
}

#[test]
fn reads_long_chains_of_operators_and_elements_without_recursion() {
    const LINKS: usize = 100_000;
    // Each operator that groups from left to right is a loop.
    let mut source = b"x = a".to_vec();
    source.extend(b" + a".repeat(LINKS));
    let (tree, _, errors) = parsed(&source);
    assert!(errors.is_empty());
    assert_eq!(tree.root().text(), source);
    let mut value = first_value(&tree);
    let mut sums = 0;
    while let Expr::Binary(binary) = value {
        sums += 1;
        value = binary.left().expect("each sum has a left operand");
    }
    assert_eq!(sums, LINKS);

    // A chain of comparisons is one node.
    let mut source = b"x = a".to_vec();
    source.extend(b" < a".repeat(LINKS));
    let (tree, _, errors) = parsed(&source);
    assert!(errors.is_empty());
    let Expr::Compare(compare) = first_value(&tree) else {
        panic!("a comparison");
    };
    assert_eq!(compare.operands().count(), LINKS + 1);

    // So are lists, tuples, tables, and f-strings, however long.
    for (open, element, close) in [
        ("[", "a, ", "]"),
        ("(", "a, ", ")"),
        ("{", "a, ", "}"),
        ("f\"", "{a}", "\""),
    ] {
        let mut source = format!("x = {open}").into_bytes();
        source.extend(element.repeat(LINKS).as_bytes());
        source.extend(close.as_bytes());
        let (tree, _, errors) = parsed(&source);
        assert!(errors.is_empty(), "{open}");
        assert_eq!(tree.root().text(), source);
        let elements = match first_value(&tree) {
            Expr::List(list) => list.elements().count(),
            Expr::Tuple(tuple) => tuple.elements().count(),
            Expr::Table(table) => table.fields().count(),
            Expr::FString(fstring) => fstring.parts().count(),
            other => panic!("a collection: {other:?}"),
        };
        assert_eq!(elements, LINKS, "{open}");
    }
}

// The precedence property test (invariant 1 of `docs/specs/language.md`). The generator follows
// the grammar of the spec's "Precedence" section, so every expression it writes is valid, and
// its choices give the grouping that the grammar implies. A separate precedence-climbing
// function, written from the spec's precedence table, groups the same tokens again, and the
// parser's tree must group them as both do.

/// The comparison operators, `not in` and `is not` written as two tokens.
const COMPARISONS: [&[&str]; 10] = [
    &["<"],
    &["<="],
    &[">"],
    &[">="],
    &["=="],
    &["!="],
    &["in"],
    &["not", "in"],
    &["is"],
    &["is", "not"],
];

/// The levels of binary operators that group from left to right, from `|` to `*`, each with
/// its operators.
const LEFT_LEVELS: [&[&str]; 6] = [
    &["|"],
    &["^"],
    &["&"],
    &["<<", ">>"],
    &["+", "-"],
    &["*", "/", "//", "%"],
];

/// Chooses an entry of `items`.
fn pick<'a, T>(source: &mut Source, items: &'a [T]) -> &'a T {
    let bound = u64::try_from(items.len()).expect("lists are short");
    let index = usize::try_from(source.below(bound)).expect("choices are below their bound");
    &items[index]
}

/// A generated expression: its tokens, and how the grammar groups them.
struct Generated {
    tokens: Vec<&'static str>,
}

impl Generated {
    /// `exp ::= orexp ['if' orexp 'else' exp]`
    fn exp(&mut self, source: &mut Source, depth: u32) -> String {
        let value = self.or_exp(source, depth);
        if depth == 0 || source.below(5) != 1 {
            return value;
        }
        self.tokens.push("if");
        let condition = self.or_exp(source, depth - 1);
        self.tokens.push("else");
        let other = self.exp(source, depth - 1);
        format!("({value} if {condition} else {other})")
    }

    /// `orexp ::= andexp {'or' andexp}`
    fn or_exp(&mut self, source: &mut Source, depth: u32) -> String {
        let mut left = self.and_exp(source, depth);
        while source.below(5) == 1 {
            self.tokens.push("or");
            let right = self.and_exp(source, depth);
            left = format!("({left} or {right})");
        }
        left
    }

    /// `andexp ::= notexp {'and' notexp}`
    fn and_exp(&mut self, source: &mut Source, depth: u32) -> String {
        let mut left = self.not_exp(source, depth);
        while source.below(5) == 1 {
            self.tokens.push("and");
            let right = self.not_exp(source, depth);
            left = format!("({left} and {right})");
        }
        left
    }

    /// `notexp ::= 'not' notexp | comparison`
    fn not_exp(&mut self, source: &mut Source, depth: u32) -> String {
        if depth > 0 && source.below(5) == 1 {
            self.tokens.push("not");
            let operand = self.not_exp(source, depth - 1);
            return format!("(not {operand})");
        }
        self.comparison(source, depth)
    }

    /// `comparison ::= bitor {compop bitor}`
    fn comparison(&mut self, source: &mut Source, depth: u32) -> String {
        let mut parts = vec![self.left_level(source, depth, 0)];
        while source.below(4) == 1 {
            let operator = pick(source, &COMPARISONS);
            self.tokens.extend_from_slice(operator);
            parts.push(operator.join(" "));
            parts.push(self.left_level(source, depth, 0));
        }
        if parts.len() == 1 {
            return parts.remove(0);
        }
        format!("({})", parts.join(" "))
    }

    /// `bitor`, `bitxor`, `bitand`, `shift`, `sum`, and `term`: each level's operands, from the
    /// next level, with one of its operators between each two.
    fn left_level(&mut self, source: &mut Source, depth: u32, level: usize) -> String {
        let Some(operators) = LEFT_LEVELS.get(level) else {
            return self.unary(source, depth);
        };
        let mut left = self.left_level(source, depth, level + 1);
        while source.below(5) == 1 {
            let operator = pick(source, operators);
            self.tokens.push(operator);
            let right = self.left_level(source, depth, level + 1);
            left = format!("({left} {operator} {right})");
        }
        left
    }

    /// `unary ::= ('-' | '+' | '~' | '#') unary | power`
    fn unary(&mut self, source: &mut Source, depth: u32) -> String {
        if depth > 0 && source.below(5) == 1 {
            let operator = pick(source, &["-", "+", "~", "#"]);
            self.tokens.push(operator);
            let operand = self.unary(source, depth - 1);
            return format!("({operator} {operand})");
        }
        self.power(source, depth)
    }

    /// `power ::= simpleexp ['**' unary]`
    fn power(&mut self, source: &mut Source, depth: u32) -> String {
        let base = self.simple(source, depth);
        if depth == 0 || source.below(4) != 1 {
            return base;
        }
        self.tokens.push("**");
        let exponent = self.unary(source, depth - 1);
        format!("({base} ** {exponent})")
    }

    /// A name, or a parenthesized expression, which groups as what it holds.
    fn simple(&mut self, source: &mut Source, depth: u32) -> String {
        if depth > 0 && source.below(6) == 1 {
            self.tokens.push("(");
            let inner = self.exp(source, depth - 1);
            self.tokens.push(")");
            return inner;
        }
        let name = *pick(source, &["a", "b", "c"]);
        self.tokens.push(name);
        name.to_owned()
    }
}

/// Generates an expression, and returns its tokens with how the grammar groups them.
fn generate_expression(source: &mut Source) -> (Vec<&'static str>, String) {
    let mut generated = Generated { tokens: Vec::new() };
    let grouping = generated.exp(source, 3);
    (generated.tokens, grouping)
}

/// Groups tokens by precedence climbing with the spec's precedence table, apart from the parser.
struct Climber<'a> {
    tokens: &'a [&'a str],
    pos: usize,
}

impl Climber<'_> {
    fn peek(&self, ahead: usize) -> &str {
        self.tokens.get(self.pos + ahead).copied().unwrap_or("")
    }

    fn take(&mut self) -> String {
        let token = self.peek(0).to_owned();
        self.pos += 1;
        token
    }

    /// The level of the binary operator at the cursor in the spec's table, and how many tokens
    /// it takes.
    fn binary(&self) -> Option<(u32, usize)> {
        Some(match (self.peek(0), self.peek(1)) {
            ("or", _) => (2, 1),
            ("and", _) => (3, 1),
            ("not", "in") | ("is", "not") => (5, 2),
            ("<" | "<=" | ">" | ">=" | "==" | "!=" | "in" | "is", _) => (5, 1),
            ("|", _) => (6, 1),
            ("^", _) => (7, 1),
            ("&", _) => (8, 1),
            ("<<" | ">>", _) => (9, 1),
            ("+" | "-", _) => (10, 1),
            ("*" | "/" | "//" | "%", _) => (11, 1),
            ("**", _) => (13, 1),
            _ => return None,
        })
    }

    /// Level 1, the conditional expression, which groups from right to left.
    fn expression(&mut self) -> String {
        let value = self.climb(0);
        if self.peek(0) != "if" {
            return value;
        }
        self.pos += 1;
        let condition = self.climb(0);
        self.pos += 1; // `else`
        let other = self.expression();
        format!("({value} if {condition} else {other})")
    }

    /// An operand and every binary operator above `limit` after it.
    fn climb(&mut self, limit: u32) -> String {
        let mut left = match self.peek(0) {
            "not" => {
                self.pos += 1;
                format!("(not {})", self.climb(4))
            }
            "-" | "+" | "~" | "#" => {
                let operator = self.take();
                format!("({operator} {})", self.climb(12))
            }
            "(" => {
                self.pos += 1;
                let inner = self.expression();
                self.pos += 1; // `)`
                inner
            }
            _ => self.take(),
        };
        while let Some((level, width)) = self.binary().filter(|&(level, _)| level > limit) {
            if level == 5 {
                let mut parts = vec![left];
                while let Some((5, width)) = self.binary() {
                    let operator: Vec<String> = (0..width).map(|_| self.take()).collect();
                    parts.push(operator.join(" "));
                    parts.push(self.climb(5));
                }
                left = format!("({})", parts.join(" "));
                continue;
            }
            let operator: Vec<String> = (0..width).map(|_| self.take()).collect();
            // `**` groups from right to left, over a unary operand.
            let right = self.climb(if level == 13 { 12 } else { level });
            left = format!("({left} {} {right})", operator.join(" "));
        }
        left
    }
}

#[test]
fn groups_every_generated_expression_as_the_grammar_and_the_precedence_table_say() {
    Config::default()
        .with_cases(3_000)
        .assert(generate_expression, |(tokens, expected)| {
            let source = format!("x = {}", tokens.join(" "));
            if !first_error(source.as_bytes()).is_empty() {
                return false;
            }
            let (tree, _, _) = parsed(source.as_bytes());
            let parsed = group(first_value(&tree).syntax());
            let mut climber = Climber { tokens, pos: 0 };
            let climbed = climber.expression();
            parsed == *expected && climbed == *expected && climber.pos == tokens.len()
        });
}
