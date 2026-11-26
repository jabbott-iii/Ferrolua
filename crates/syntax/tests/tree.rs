//! Tests of the lossless tree through the public API: building it from events, each list of
//! events it rejects, walking and printing it, its typed views, and the round trip from the tree
//! back to the source and its events on generated input.

use lunaferra_syntax::ast::{AstNode, Block, Chunk, ErrorNode};
use lunaferra_syntax::{
    Error, Event, Lexed, Limits, NodeKind, Origin, SourceId, Span, SyntaxElement, SyntaxNode,
    SyntaxTree, Token, TokenKind, TriviaKind, lex,
};
use lunaferra_testkit::{Config, Source};

use NodeKind as N;
use TokenKind as T;

/// Lexes `source` as a string, as `load` reads one.
fn lexed(source: &[u8]) -> Lexed {
    lex(source, SourceId::new(0), Origin::String, &Limits::default())
        .expect("each test's source is short")
}

/// Returns the event that names `token`.
fn token(token: &Token) -> Event {
    Event::Token(token.kind, token.span)
}

/// Returns the events of a chunk whose block holds every token but the last, `Eof`, which
/// follows the block.
fn flat_events(lexed: &Lexed) -> Vec<Event> {
    let (eof, rest) = lexed
        .tokens()
        .split_last()
        .expect("the last token is always `Eof`");
    let mut events = vec![Event::Start(N::Chunk), Event::Start(N::Block)];
    events.extend(rest.iter().map(token));
    events.extend([Event::Finish, token(eof), Event::Finish]);
    events
}

/// Builds the tree of `source`, lexed as a string, from `events`.
fn build(source: &[u8], events: &[Event]) -> Result<SyntaxTree, Error> {
    SyntaxTree::build(source, &lexed(source), events)
}

/// Builds the tree of `source`, lexed as a string, from its flat events.
fn flat_tree(source: &[u8]) -> SyntaxTree {
    let lexed = lexed(source);
    SyntaxTree::build(source, &lexed, &flat_events(&lexed)).expect("flat events make a tree")
}

/// Walks `tree` and returns the events that describe it, without recursion.
///
/// The walk stops once it has more than `most` events, so a tree that walks wrongly, for
/// example by visiting a node again and again, fails a comparison quickly instead of filling
/// memory.
fn events_of(tree: &SyntaxTree, most: usize) -> Vec<Event> {
    let root = tree.root();
    let mut events = vec![Event::Start(root.kind())];
    let mut open = vec![root.children()];
    while events.len() <= most
        && let Some(children) = open.last_mut()
    {
        match children.next() {
            Some(SyntaxElement::Node(node)) => {
                events.push(Event::Start(node.kind()));
                open.push(node.children());
            }
            Some(SyntaxElement::Token(token)) => {
                events.push(Event::Token(token.kind(), token.span()));
            }
            None => {
                events.push(Event::Finish);
                open.pop();
            }
        }
    }
    events
}

/// Returns the kinds of `node`'s children, with `None` for each token.
fn child_kinds(node: SyntaxNode<'_>) -> Vec<Option<NodeKind>> {
    node.children()
        .map(|child| child.into_node().map(|node| node.kind()))
        .collect()
}

#[test]
fn gives_back_a_program_with_comments_and_blank_lines_from_its_tree() {
    let source = b"#!x\n-- a comment\nlocal x = 1 --[[ long\n comment ]] \r\n\n\tprint(x)  \n";
    let lexed = lexed(source);
    let tree = SyntaxTree::build(source, &lexed, &flat_events(&lexed)).unwrap();
    assert_eq!(tree.root().text(), source);
    assert_eq!(tree.source(), source);
}

#[test]
fn gives_back_an_empty_source_from_its_tree() {
    let tree = flat_tree(b"");
    assert_eq!(tree.root().text(), b"");
    let events = flat_events(&lexed(b""));
    assert_eq!(events_of(&tree, events.len()), events);
}

#[test]
fn gives_back_a_source_of_lexical_errors_from_its_tree() {
    let source = b"x = 0x @ \"\\q\" [==[ \xFF";
    let lexed = lexed(source);
    assert!(!lexed.errors().is_empty());
    let tree = SyntaxTree::build(source, &lexed, &flat_events(&lexed)).unwrap();
    assert_eq!(tree.root().text(), source);
}

#[test]
fn builds_the_nodes_and_tokens_that_the_events_describe() {
    // `x = 1`, with `=` in an error node and an empty block before the end.
    let source = b"x = 1";
    let lexed = lexed(source);
    let [x, equal, one, eof] = lexed.tokens() else {
        panic!("`x = 1` is three tokens, then the end");
    };
    let events = [
        Event::Start(N::Chunk),
        Event::Start(N::Block),
        token(x),
        Event::Start(N::Error),
        token(equal),
        Event::Finish,
        token(one),
        Event::Finish,
        Event::Start(N::Block),
        Event::Finish,
        token(eof),
        Event::Finish,
    ];
    let tree = SyntaxTree::build(source, &lexed, &events).unwrap();
    let root = tree.root();
    assert_eq!(root.kind(), N::Chunk);
    assert_eq!(child_kinds(root), [Some(N::Block), Some(N::Block), None]);

    let mut children = root.children();
    let block = children.next().unwrap().into_node().unwrap();
    assert_eq!(child_kinds(block), [None, Some(N::Error), None]);
    assert_eq!(block.text(), b"x = 1");
    let tokens: Vec<_> = block
        .children()
        .filter_map(SyntaxElement::into_token)
        .map(|token| (token.kind(), token.text()))
        .collect();
    assert_eq!(tokens, [(T::Name, &b"x"[..]), (T::Integer, &b"1"[..])]);

    // The space after `x` is the trailing trivia of `x`, and the one after `=` that of `=`.
    let error = block.children().nth(1).unwrap().into_node().unwrap();
    assert_eq!(error.text(), b"= ");

    let empty = children.next().unwrap().into_node().unwrap();
    assert_eq!(empty.children().count(), 0);
    assert_eq!(empty.text(), b"");

    let end = children.next().unwrap().into_token().unwrap();
    assert_eq!(end.kind(), T::Eof);
    assert_eq!(children.next(), None);
    assert_eq!(events_of(&tree, events.len()), events);
}

#[test]
fn gives_each_token_its_bytes_span_and_trivia() {
    let source = b"-- note\n  x -- after\n";
    let tree = flat_tree(source);
    let chunk = Chunk::cast(tree.root()).unwrap();
    let block = chunk.block().unwrap();
    let x = block
        .syntax()
        .children()
        .next()
        .unwrap()
        .into_token()
        .unwrap();
    assert_eq!(x.kind(), T::Name);
    assert_eq!(x.text(), b"x");
    assert_eq!(
        x.span(),
        Span {
            source: SourceId::new(0),
            start: 10,
            end: 11
        }
    );
    let leading: Vec<_> = x.leading().iter().map(|trivia| trivia.kind).collect();
    assert_eq!(
        leading,
        [
            TriviaKind::Comment,
            TriviaKind::LineBreak,
            TriviaKind::Whitespace
        ]
    );
    let trailing: Vec<_> = x.trailing().iter().map(|trivia| trivia.kind).collect();
    assert_eq!(trailing, [TriviaKind::Whitespace, TriviaKind::Comment]);

    let eof = chunk.eof().unwrap();
    assert_eq!(eof.text(), b"");
    assert_eq!(eof.leading().len(), 1);
    assert_eq!(eof.trailing(), []);
}

#[test]
fn includes_each_tokens_trivia_in_a_nodes_text() {
    let tree = flat_tree(b"a -- one\n\nb --[[ two ]]");
    let block = Chunk::cast(tree.root()).unwrap().block().unwrap();
    assert_eq!(block.syntax().text(), b"a -- one\n\nb --[[ two ]]");
}

#[test]
fn reads_no_bytes_from_a_source_other_than_the_one_lexed() {
    let lexed = lexed(b"x = 1");
    let tree = SyntaxTree::build(b"", &lexed, &flat_events(&lexed)).unwrap();
    assert_eq!(tree.root().text(), b"");
    let x = tree.root().children().next().unwrap().into_node().unwrap();
    let x = x.children().next().unwrap().into_token().unwrap();
    assert_eq!(x.text(), b"");
}

#[test]
fn rejects_a_token_before_the_root() {
    let lexed = lexed(b"");
    let eof = lexed.tokens()[0];
    let events = [token(&eof), Event::Start(N::Chunk), Event::Finish];
    assert_eq!(
        build(b"", &events),
        Err(Error::EventOutsideRoot { event: 0 })
    );
}

#[test]
fn rejects_a_finish_with_no_node_open() {
    assert_eq!(
        build(b"", &[Event::Finish]),
        Err(Error::EventOutsideRoot { event: 0 })
    );
    let mut events = flat_events(&lexed(b""));
    events.push(Event::Finish);
    assert_eq!(
        build(b"", &events),
        Err(Error::EventOutsideRoot { event: 5 })
    );
}

#[test]
fn rejects_a_second_root() {
    let mut events = flat_events(&lexed(b""));
    events.extend([Event::Start(N::Chunk), Event::Finish]);
    assert_eq!(
        build(b"", &events),
        Err(Error::EventOutsideRoot { event: 5 })
    );
}

#[test]
fn rejects_a_token_after_the_root() {
    let lexed = lexed(b"x");
    let [x, eof] = lexed.tokens() else {
        panic!("`x` is one token, then the end");
    };
    let events = [Event::Start(N::Chunk), token(x), Event::Finish, token(eof)];
    assert_eq!(
        build(b"x", &events),
        Err(Error::EventOutsideRoot { event: 3 })
    );
}

#[test]
fn rejects_a_token_of_another_kind() {
    let lexed = lexed(b"x");
    let x = lexed.tokens()[0];
    let events = [Event::Start(N::Chunk), Event::Token(T::String, x.span)];
    assert_eq!(
        build(b"x", &events),
        Err(Error::TokenOutOfOrder { event: 1 })
    );
}

#[test]
fn rejects_a_token_with_another_span() {
    let lexed = lexed(b"xy");
    let xy = lexed.tokens()[0];
    let short = Span { end: 1, ..xy.span };
    let other_source = Span {
        source: SourceId::new(1),
        ..xy.span
    };
    for span in [short, other_source] {
        let events = [Event::Start(N::Chunk), Event::Token(T::Name, span)];
        assert_eq!(
            build(b"xy", &events),
            Err(Error::TokenOutOfOrder { event: 1 }),
            "{span:?}"
        );
    }
}

#[test]
fn rejects_a_token_named_out_of_order() {
    let lexed = lexed(b"x y");
    let [x, y, eof] = lexed.tokens() else {
        panic!("`x y` is two tokens, then the end");
    };
    let skipped = [Event::Start(N::Chunk), token(y), token(x)];
    assert_eq!(
        build(b"x y", &skipped),
        Err(Error::TokenOutOfOrder { event: 1 })
    );
    let twice = [Event::Start(N::Chunk), token(x), token(x)];
    assert_eq!(
        build(b"x y", &twice),
        Err(Error::TokenOutOfOrder { event: 2 })
    );
    let past_the_end = [
        Event::Start(N::Chunk),
        token(x),
        token(y),
        token(eof),
        token(eof),
    ];
    assert_eq!(
        build(b"x y", &past_the_end),
        Err(Error::TokenOutOfOrder { event: 4 })
    );
}

#[test]
fn rejects_events_that_end_early() {
    let lexed = lexed(b"x");
    let [x, eof] = lexed.tokens() else {
        panic!("`x` is one token, then the end");
    };
    let none: [Event; 0] = [];
    let open = [Event::Start(N::Chunk), token(x), token(eof)];
    let unnamed = [Event::Start(N::Chunk), token(x), Event::Finish];
    let nothing_named = [Event::Start(N::Chunk), Event::Finish];
    for events in [&none[..], &open, &unnamed, &nothing_named] {
        assert_eq!(
            build(b"x", events),
            Err(Error::EventsEndEarly),
            "{events:?}"
        );
    }
}

#[test]
fn describes_each_error_in_the_events() {
    let messages = [
        (
            Error::EventOutsideRoot { event: 3 },
            "event 3 falls outside the root node",
        ),
        (
            Error::TokenOutOfOrder { event: 7 },
            "event 7 doesn't name the source's next token",
        ),
        (
            Error::EventsEndEarly,
            "the events end before every node is finished and every token is named",
        ),
    ];
    for (error, message) in messages {
        assert_eq!(error.to_string(), message);
    }
}

#[test]
fn builds_walks_and_prints_a_tree_nested_100_000_levels_deep() {
    let depth = 100_000;
    let source = b"x";
    let lexed = lexed(source);
    let [x, eof] = lexed.tokens() else {
        panic!("`x` is one token, then the end");
    };
    let mut events = vec![Event::Start(N::Chunk)];
    events.extend(std::iter::repeat_n(Event::Start(N::Error), depth));
    events.push(token(x));
    events.extend(std::iter::repeat_n(Event::Finish, depth));
    events.extend([token(eof), Event::Finish]);
    let tree = SyntaxTree::build(source, &lexed, &events).unwrap();
    assert_eq!(tree.root().text(), source);
    assert_eq!(events_of(&tree, events.len()), events);

    // Going down one level at a time, no more than once past the expected depth, finds `x` at
    // the bottom.
    let mut node = tree.root();
    let mut levels = 0;
    while levels <= depth
        && let Some(child) = node.children().next().and_then(SyntaxElement::into_node)
    {
        node = child;
        levels += 1;
    }
    assert_eq!(levels, depth);
    assert_eq!(node.text(), b"x");
    // Dropping the tree must not recurse either.
    drop(tree);
}

#[test]
fn steps_over_a_child_nodes_descendants() {
    // A block whose first child holds 1,000 tokens, then one more token.
    let source = "a ".repeat(1_001);
    let lexed = lexed(source.as_bytes());
    let (eof, rest) = lexed.tokens().split_last().unwrap();
    let (last, inner) = rest.split_last().unwrap();
    let mut events = vec![Event::Start(N::Chunk), Event::Start(N::Error)];
    events.extend(inner.iter().map(token));
    events.extend([Event::Finish, token(last), token(eof), Event::Finish]);
    let tree = SyntaxTree::build(source.as_bytes(), &lexed, &events).unwrap();
    assert_eq!(child_kinds(tree.root()), [Some(N::Error), None, None]);
    let error = tree.root().children().next().unwrap().into_node().unwrap();
    assert_eq!(error.children().count(), 1_000);
}

#[test]
fn tells_nodes_and_tokens_apart_by_their_tree_and_place() {
    let tree = flat_tree(b"x y");
    let copy = tree.clone();
    assert_eq!(tree, copy);

    let root = tree.root();
    assert_eq!(root, tree.root());
    assert_ne!(root, copy.root());
    let block = root.children().next().unwrap().into_node().unwrap();
    assert_ne!(root, block);

    let tokens: Vec<_> = block
        .children()
        .filter_map(SyntaxElement::into_token)
        .collect();
    let copied: Vec<_> = copy
        .root()
        .children()
        .next()
        .unwrap()
        .into_node()
        .unwrap()
        .children()
        .filter_map(SyntaxElement::into_token)
        .collect();
    assert_eq!(tokens[0], tokens[0]);
    assert_ne!(tokens[0], tokens[1]);
    assert_ne!(tokens[0], copied[0]);
}

#[test]
fn shows_a_node_and_a_token_by_kind_and_place() {
    let tree = flat_tree(b"x");
    let block = tree.root().children().next().unwrap();
    assert_eq!(
        format!("{block:?}"),
        "Node(SyntaxNode { kind: Block, position: 1 })"
    );
    let x = block.into_node().unwrap().children().next().unwrap();
    assert_eq!(
        format!("{x:?}"),
        "Token(SyntaxToken { kind: Name, span: Span { source: SourceId(0), start: 0, end: 1 } })"
    );
}

#[test]
fn gives_an_element_as_a_node_or_a_token() {
    let tree = flat_tree(b"x");
    let [block, eof]: [SyntaxElement<'_>; 2] = tree
        .root()
        .children()
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    assert_eq!(block.into_node().map(|node| node.kind()), Some(N::Block));
    assert_eq!(block.into_token(), None);
    assert_eq!(eof.into_node(), None);
    assert_eq!(eof.into_token().map(|token| token.kind()), Some(T::Eof));
}

#[test]
fn casts_a_node_only_to_the_view_of_its_kind() {
    let source = b"x";
    let lexed = lexed(source);
    let [x, eof] = lexed.tokens() else {
        panic!("`x` is one token, then the end");
    };
    let events = [
        Event::Start(N::Chunk),
        Event::Start(N::Error),
        token(x),
        Event::Finish,
        Event::Start(N::Block),
        Event::Finish,
        token(eof),
        Event::Finish,
    ];
    let tree = SyntaxTree::build(source, &lexed, &events).unwrap();
    let nodes: Vec<_> = tree
        .root()
        .children()
        .filter_map(SyntaxElement::into_node)
        .collect();
    let [error, block] = nodes[..] else {
        panic!("the chunk holds two nodes");
    };
    let root = tree.root();

    assert_eq!(Chunk::cast(root).map(|view| view.syntax()), Some(root));
    assert_eq!(Chunk::cast(block), None);
    assert_eq!(Block::cast(block).map(|view| view.syntax()), Some(block));
    assert_eq!(Block::cast(error), None);
    assert_eq!(
        ErrorNode::cast(error).map(|view| view.syntax()),
        Some(error)
    );
    assert_eq!(ErrorNode::cast(root), None);

    // The chunk's block is its first child of the block's kind, after the error node.
    let chunk = Chunk::cast(root).unwrap();
    assert_eq!(chunk.block(), Block::cast(block));
    assert_eq!(chunk.eof().map(|token| token.span()), Some(eof.span));
}

#[test]
fn gives_no_part_that_a_chunk_lacks() {
    let lexed = lexed(b"");
    let eof = lexed.tokens()[0];
    // A chunk with no block, and a chunk whose end of the source is inside its block.
    let no_block = [Event::Start(N::Chunk), token(&eof), Event::Finish];
    let tree = build(b"", &no_block).unwrap();
    let chunk = Chunk::cast(tree.root()).unwrap();
    assert_eq!(chunk.block(), None);
    assert!(chunk.eof().is_some());

    let end_inside = [
        Event::Start(N::Chunk),
        Event::Start(N::Block),
        token(&eof),
        Event::Finish,
        Event::Finish,
    ];
    let tree = build(b"", &end_inside).unwrap();
    let chunk = Chunk::cast(tree.root()).unwrap();
    assert!(chunk.block().is_some());
    assert_eq!(chunk.eof(), None);
}

#[test]
fn gives_back_every_conformance_script_and_its_twin_from_its_tree() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
    let mut scripts = 0;
    for folder in [folder.clone(), folder.join("reference")] {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "lua") {
                let source = std::fs::read(&path).unwrap();
                let lexed =
                    lex(&source, SourceId::new(0), Origin::File, &Limits::default()).unwrap();
                let tree = SyntaxTree::build(&source, &lexed, &flat_events(&lexed)).unwrap();
                assert_eq!(tree.root().text(), source, "{}", path.display());
                scripts += 1;
            }
        }
    }
    assert!(scripts >= 4, "found {scripts} scripts");
}

/// Bytes that start or end tokens and trivia, which generated sources are made of, so that
/// most sources hold strings, comments, numerals, f-strings, and errors.
const ALPHABET: &[u8] = b"af0x9ep.\"'[]=-{}:!\\\n\r #()zurs;\x00\xFF";

/// The node kinds that generated trees use.
const KINDS: [NodeKind; 3] = [N::Chunk, N::Block, N::Error];

/// Chooses a number below `bound`, as a `usize`.
fn below(source: &mut Source, bound: usize) -> usize {
    let bound = u64::try_from(bound).expect("bounds are small");
    usize::try_from(source.below(bound)).expect("choices are below their bound")
}

/// Generates a source of up to about 40 bytes, most of them from [`ALPHABET`].
fn generate_source(source: &mut Source) -> Vec<u8> {
    let mut bytes = Vec::new();
    while source.below(40) != 0 {
        let byte = match ALPHABET.get(below(source, ALPHABET.len() + 1)) {
            Some(&byte) => byte,
            None => u8::try_from(source.below(256)).expect("a choice below 256 is a byte"),
        };
        bytes.push(byte);
    }
    bytes
}

/// Generates the events of a tree over `tokens`: a root of any kind, holding nodes of any kind
/// nested to any depth, some of them empty, with the tokens in order among them.
fn generate_events(source: &mut Source, tokens: &[Token]) -> Vec<Event> {
    let mut events = vec![Event::Start(KINDS[below(source, KINDS.len())])];
    let mut depth = 1;
    for next in tokens {
        loop {
            match source.below(4) {
                1 => {
                    events.push(Event::Start(KINDS[below(source, KINDS.len())]));
                    depth += 1;
                }
                2 if depth > 1 => {
                    events.push(Event::Finish);
                    depth -= 1;
                }
                _ => break,
            }
        }
        events.push(token(next));
    }
    events.extend(std::iter::repeat_n(Event::Finish, depth));
    events
}

#[test]
fn gives_back_every_generated_source_and_its_events_from_its_tree() {
    Config::default().with_cases(2_000).assert(
        |source| {
            let bytes = generate_source(source);
            let origin = [Origin::String, Origin::File][below(source, 2)];
            let lexed = lex(&bytes, SourceId::new(0), origin, &Limits::default()).unwrap();
            let events = generate_events(source, lexed.tokens());
            (bytes, origin, events)
        },
        |(bytes, origin, events)| {
            let lexed = lex(bytes, SourceId::new(0), *origin, &Limits::default()).unwrap();
            let tree = SyntaxTree::build(bytes, &lexed, events).map_err(|e| e.to_string())?;
            if tree.root().text() != *bytes {
                return Err(String::from("the root's text isn't the source"));
            }
            if events_of(&tree, events.len()) != *events {
                return Err(String::from("walking the tree gives other events"));
            }
            Ok(())
        },
    );
}

#[test]
fn gives_each_node_the_text_of_its_children_in_order() {
    Config::default().with_cases(500).assert(
        |source| {
            let bytes = generate_source(source);
            let events = generate_events(source, lexed(&bytes).tokens());
            (bytes, events)
        },
        |(bytes, events)| {
            let tree = build(bytes, events).unwrap();
            let mut nodes = vec![tree.root()];
            let mut visited = 0;
            while let Some(node) = nodes.pop() {
                // Each node comes from one `Start` event, so a walk that visits more nodes
                // than there are events is wrong, and stops here.
                visited += 1;
                if visited > events.len() {
                    return Err(String::from("the walk visits more nodes than there are"));
                }
                let mut joined = Vec::new();
                for child in node.children() {
                    match child {
                        SyntaxElement::Node(child) => {
                            joined.extend(child.text());
                            nodes.push(child);
                        }
                        SyntaxElement::Token(token) => {
                            // A token's trivia lies next to it, so its bytes with its trivia
                            // run from its first leading trivia to its last trailing trivia.
                            let first = token.leading().first().map(|trivia| trivia.span);
                            let last = token.trailing().last().map(|trivia| trivia.span);
                            let start = first.unwrap_or(token.span()).start;
                            let end = last.unwrap_or(token.span()).end;
                            let start = usize::try_from(start).unwrap();
                            let end = usize::try_from(end).unwrap();
                            joined.extend_from_slice(&bytes[start..end]);
                        }
                    }
                }
                if node.text() != joined {
                    return Err(format!("{node:?}"));
                }
            }
            Ok(())
        },
    );
}

/// Whether `events` make a tree over `tokens`, by the rule [`SyntaxTree::build`] documents,
/// worked out a second way: one root, every event inside it, each `Finish` closing an open node,
/// and the tokens named exactly once each, in order.
fn makes_a_tree(events: &[Event], tokens: &[Token]) -> bool {
    let mut depth = 0_usize;
    let mut roots = 0;
    let mut named = Vec::new();
    for event in events {
        match *event {
            Event::Start(_) => {
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
            }
            Event::Token(kind, span) => {
                if depth == 0 {
                    return false;
                }
                named.push((kind, span));
            }
            Event::Finish => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ => unreachable!("the test knows every kind of event"),
        }
    }
    let all: Vec<_> = tokens
        .iter()
        .map(|token| (token.kind, token.span))
        .collect();
    roots == 1 && depth == 0 && named == all
}

/// Generates the events of a tree over `tokens`, then, in most cases, changes them in one of
/// the ways that can break a tree: an event removed, added, or swapped with the next, or a
/// token's kind or span changed.
fn generate_changed_events(source: &mut Source, tokens: &[Token]) -> Vec<Event> {
    let mut events = generate_events(source, tokens);
    let at = below(source, events.len() + 1);
    match source.below(6) {
        1 if at < events.len() => {
            events.remove(at);
        }
        2 => {
            let added = match source.below(3) {
                0 => Event::Start(N::Error),
                1 => Event::Finish,
                _ => token(&tokens[below(source, tokens.len())]),
            };
            events.insert(at, added);
        }
        3 if at + 1 < events.len() => events.swap(at, at + 1),
        4 | 5 => {
            if let Some(Event::Token(kind, span)) = events.get_mut(at) {
                if source.below(2) == 0 {
                    *kind = if *kind == T::Name { T::String } else { T::Name };
                } else {
                    span.end += 1;
                }
            }
        }
        _ => {}
    }
    events
}

#[test]
fn accepts_exactly_the_lists_of_events_that_make_a_tree() {
    Config::default().with_cases(3_000).assert(
        |source| {
            let bytes = generate_source(source);
            let events = generate_changed_events(source, lexed(&bytes).tokens());
            (bytes, events)
        },
        |(bytes, events)| {
            let lexed = lexed(bytes);
            let expected = makes_a_tree(events, lexed.tokens());
            match SyntaxTree::build(bytes, &lexed, events) {
                Ok(tree) if expected => {
                    if events_of(&tree, events.len()) == *events {
                        Ok(())
                    } else {
                        Err(String::from("walking the tree gives other events"))
                    }
                }
                Ok(_) => Err(String::from("built a tree from events that make none")),
                Err(error) if expected => Err(format!("rejected events that make a tree: {error}")),
                Err(_) => Ok(()),
            }
        },
    );
}
