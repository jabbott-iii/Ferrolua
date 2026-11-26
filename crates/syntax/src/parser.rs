//! The parser: [`parse`] reads the tokens of a source and returns the [`Event`]s of its tree,
//! which [`SyntaxTree::build`](crate::SyntaxTree::build) turns into the lossless tree.
//!
//! It is hand-written, with recursive descent, and follows PUC-Rio Lua 5.5.1's `lparser.c` rule
//! by rule, so it accepts the same statements and reports each mistake with the same message and
//! line. It reads every statement, and the expressions that statements are built from; the rest
//! of the expressions join it in M1.5. The statements and the expressions are modules of their
//! own, and this module holds what both use: the cursor over the tokens, the markers that open
//! and close nodes, and the errors.
//!
//! # Markers
//!
//! A node's `Start` event must come before its first token, but the parser often learns what a
//! node is only after reading part of it: `t.x` is a field only once the `.` follows `t`, and
//! `f(x)` is a call statement only once no `=` follows it. So the parser writes its events to a
//! list of its own first. Starting a node writes a placeholder that completing it fills with the
//! node's kind, and the parser can start a node before a node it has already completed, which
//! sets a link from the completed node's placeholder to the new one. When the parse is done,
//! [`Parser::into_events`] writes each placeholder's `Start`, preceded by those of the nodes
//! linked to it, so each node opens before what it holds. Every step takes constant time, so a
//! chain of a million fields takes time in proportion to its length, not its square.

mod error;
mod expression;
mod statement;

pub use error::{ParseError, ParseErrorKind};

use crate::lexer::string_as_read;
use crate::{Event, Lexed, Near, NodeKind, Span, Token, TokenKind};

/// Reads the tokens of `source`, which `lexed` holds, and returns the events of its tree.
///
/// `lexed` must be what [`lex`](crate::lex) gave for `source`. With any other source, the error
/// messages quote the wrong text, though nothing panics.
///
/// The parser stops at the source's first error, as PUC-Rio Lua does. A syntax error comes as an
/// [`Event::Error`] when it comes before the token where PUC-Rio's lexer would find the source's
/// first lexical error, which [`Lexed::errors`] holds; at or past that token, the parser stops
/// without one, because PUC-Rio reports the lexical error first. Either way, the tokens from
/// where it stopped to the end of the source, apart from the `Eof` token, go in a
/// [`NodeKind::Error`] node in the node it was reading, and each node it had started is
/// finished as far as it got. So the events always make a tree over every token, which
/// [`SyntaxTree::build`](crate::SyntaxTree::build) accepts.
///
/// # Examples
///
/// ```
/// use lunaferra_syntax::{Event, Origin, SourceId, SyntaxTree, lex, parse};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = b"local x = 1\nif x then print(x) end\n";
/// let lexed = lex(source, SourceId::new(0), Origin::String)?;
/// let events = parse(source, &lexed);
/// let tree = SyntaxTree::build(source, &lexed, &events)?;
/// assert_eq!(tree.root().text(), source);
///
/// let broken = b"if x then\nprint(x)\n";
/// let lexed = lex(broken, SourceId::new(1), Origin::String)?;
/// let errors: Vec<String> = parse(broken, &lexed)
///     .iter()
///     .filter_map(|event| match event {
///         Event::Error(error) => Some(format!("{}: {error}", error.line)),
///         _ => None,
///     })
///     .collect();
/// assert_eq!(errors, ["3: 'end' expected (to close 'if' at line 1) near <eof>"]);
/// # Ok(())
/// # }
/// ```
#[must_use]
pub fn parse(source: &[u8], lexed: &Lexed) -> Vec<Event> {
    let mut parser = Parser::new(source, lexed);
    parser.chunk();
    parser.into_events()
}

/// The parser's state: where it is in the tokens, the events it has written, and the functions
/// it is inside.
struct Parser<'s> {
    /// The source's bytes, which the error messages quote.
    source: &'s [u8],
    /// The source's tokens and line map.
    lexed: &'s Lexed,
    /// The position of the current token in `lexed.tokens()`.
    pos: usize,
    /// The position of the last token, which is always [`TokenKind::Eof`].
    eof: usize,
    /// The position of the token where PUC-Rio's lexer would find the source's first lexical
    /// error, if it has one: the first token that ends after the error starts. The parser
    /// reports no syntax error at or past it.
    lex_stop: Option<usize>,
    /// Whether the parser has stopped at an error.
    stopped: bool,
    /// The events so far, in the parser's own form.
    events: Vec<Raw>,
    /// One entry for each node started, in the order started.
    nodes: Vec<Node>,
    /// One entry for each function the parser is inside, the innermost last. The first is the
    /// main chunk's.
    functions: Vec<Function>,
}

/// A function the parser is inside.
#[derive(Clone, Copy, Debug)]
struct Function {
    /// Whether `...` is among its parameters, so that `...` may appear in its body. The main
    /// chunk's is always true.
    vararg: bool,
    /// How many loops of this function hold the current token, so that `break` may appear.
    loops: u32,
}

/// One of the parser's own events.
#[derive(Clone, Debug)]
enum Raw {
    /// Starts the node at this position in [`Parser::nodes`].
    Start(usize),
    /// Adds the token at this position in the tokens.
    Token(usize),
    /// Finishes the node that is open.
    Finish,
    /// Reports an error.
    Error(ParseError),
}

/// A node the parser has started.
#[derive(Clone, Copy, Debug)]
struct Node {
    /// The node's kind, once it is completed. A node abandoned before then has none, and adds
    /// nothing to the tree: what it held belongs to the node around it.
    kind: Option<NodeKind>,
    /// The node started later, around this one, if there is one.
    parent: Option<usize>,
    /// Whether the node was started around a node already completed, which it holds.
    wraps: bool,
    /// Whether [`Parser::into_events`] has written this node's `Start`.
    written: bool,
}

/// A node that is started but not completed.
#[must_use = "a started node must be completed or abandoned"]
#[derive(Debug)]
struct Marker(usize);

/// A completed node: its position in [`Parser::nodes`], and its kind.
#[derive(Clone, Copy, Debug)]
struct Done {
    node: usize,
    kind: NodeKind,
}

impl<'s> Parser<'s> {
    fn new(source: &'s [u8], lexed: &'s Lexed) -> Self {
        let tokens = lexed.tokens();
        let eof = tokens.len().saturating_sub(1);
        let lex_stop = lexed.errors().first().map(|error| {
            tokens
                .iter()
                .position(|token| token.span.end > error.span.start)
                .unwrap_or(eof)
        });
        Self {
            source,
            lexed,
            pos: 0,
            eof,
            lex_stop,
            stopped: false,
            events: Vec::with_capacity(tokens.len().saturating_mul(2)),
            nodes: Vec::with_capacity(tokens.len()),
            functions: vec![Function {
                vararg: true,
                loops: 0,
            }],
        }
    }

    /// Reads the whole source: a chunk, which holds a block and the end of the source.
    fn chunk(&mut self) {
        let chunk = self.start();
        let block = self.start();
        self.statements();
        self.complete(block, NodeKind::Block);
        if !self.at(TokenKind::Eof) {
            self.error(ParseErrorKind::Expected(TokenKind::Eof));
        }
        self.events.push(Raw::Token(self.eof));
        self.complete(chunk, NodeKind::Chunk);
    }

    // The cursor.

    /// Returns the current token.
    fn token(&self) -> Token {
        self.token_at(self.pos)
    }

    /// Returns the token at `pos`, or the last token, `Eof`, for a position past it.
    fn token_at(&self, pos: usize) -> Token {
        let tokens = self.lexed.tokens();
        // `lex` always gives at least the `Eof` token, so the list isn't empty.
        tokens[pos.min(self.eof)]
    }

    /// Returns the current token's kind.
    fn current(&self) -> TokenKind {
        self.token().kind
    }

    /// Returns the kind of the token after the current one.
    fn next_kind(&self) -> TokenKind {
        self.token_at(self.pos.saturating_add(1)).kind
    }

    /// Returns whether the current token is of `kind`.
    fn at(&self, kind: TokenKind) -> bool {
        self.current() == kind
    }

    /// Returns the bytes of `token`.
    fn text(&self, token: Token) -> &'s [u8] {
        let start = crate::lexer::index(token.span.start);
        let end = crate::lexer::index(token.span.end);
        self.source.get(start..end).unwrap_or(&[])
    }

    /// Returns the line where `token` ends, which is the line PUC-Rio's lexer is on once it has
    /// read the token.
    fn end_line(&self, token: Token) -> u32 {
        self.lexed.lines().line_col(token.span.end).0
    }

    /// Returns the line where the current token ends.
    fn line(&self) -> u32 {
        self.end_line(self.token())
    }

    /// Adds the current token to the node that is open, and moves to the next one.
    ///
    /// The grammar calls it only once the current token has matched a kind other than
    /// [`TokenKind::Eof`], so it never adds the `Eof` token, which only the chunk holds.
    fn bump(&mut self) {
        self.events.push(Raw::Token(self.pos));
        self.pos = self.pos.saturating_add(1);
    }

    /// Adds the current token and moves on if it is of `kind`, and returns whether it was.
    fn eat(&mut self, kind: TokenKind) -> bool {
        let found = self.at(kind);
        if found {
            self.bump();
        }
        found
    }

    /// Adds the current token and moves on if it is of `kind`, and otherwise reports that a
    /// token of `kind` was expected, as PUC-Rio's `checknext` does.
    fn expect(&mut self, kind: TokenKind) {
        if !self.eat(kind) {
            self.error(ParseErrorKind::Expected(kind));
        }
    }

    /// Adds the current token and moves on if it is of `kind`, which closes the construct that
    /// `opener` opened on `line`, and otherwise reports it missing, as PUC-Rio's `check_match`
    /// does: naming the opener only when it is on another line.
    fn expect_closing(&mut self, kind: TokenKind, opener: TokenKind, line: u32) {
        if self.eat(kind) {
            return;
        }
        if line == self.line() {
            self.error(ParseErrorKind::Expected(kind));
        } else {
            self.error(ParseErrorKind::Unclosed {
                expected: kind,
                opener,
                line,
            });
        }
    }

    // The functions the parser is inside.

    /// Returns the innermost function the parser is inside.
    fn function(&self) -> Function {
        // The main chunk's entry is never removed.
        self.functions[self.functions.len().saturating_sub(1)]
    }

    /// Returns the innermost function's entry, to change it.
    fn function_mut(&mut self) -> &mut Function {
        let last = self.functions.len().saturating_sub(1);
        &mut self.functions[last]
    }

    // Markers.

    /// Starts a node, whose kind [`Parser::complete`] gives.
    fn start(&mut self) -> Marker {
        let node = self.nodes.len();
        self.nodes.push(Node {
            kind: None,
            parent: None,
            wraps: false,
            written: false,
        });
        self.events.push(Raw::Start(node));
        Marker(node)
    }

    /// Completes the node `marker` started, as a node of `kind`.
    ///
    /// Once the parser has stopped, a node that holds nothing is left out of the tree, so that
    /// the parts a broken statement lacks are missing, not empty.
    fn complete(&mut self, marker: Marker, kind: NodeKind) -> Done {
        let done = Done {
            node: marker.0,
            kind,
        };
        // Every marker comes from `start`, which pushed its node.
        let node = &mut self.nodes[marker.0];
        let empty = matches!(self.events.last(), Some(&Raw::Start(last)) if last == marker.0);
        if self.stopped && empty && !node.wraps {
            self.events.pop();
            return done;
        }
        node.kind = Some(kind);
        self.events.push(Raw::Finish);
        done
    }

    /// Abandons the node `marker` started: it adds nothing to the tree, and what it holds
    /// belongs to the node around it.
    fn abandon(&mut self, marker: Marker) {
        // The node keeps no kind, which marks it abandoned, and gets no `Finish`.
        let Marker(_) = marker;
    }

    /// Starts a node before `done`, a node already completed, so that the new node holds it.
    fn precede(&mut self, done: Done) -> Marker {
        let marker = self.start();
        // `done` comes from `complete`, and `marker` from `start`, so their nodes exist.
        self.nodes[done.node].parent = Some(marker.0);
        self.nodes[marker.0].wraps = true;
        marker
    }

    // Errors.

    /// Reports `kind` near the current token, on the line where it ends, as PUC-Rio's
    /// `luaX_syntaxerror` does, and stops.
    fn error(&mut self, kind: ParseErrorKind) {
        if self.stops_quietly() {
            return;
        }
        let token = self.token();
        let error = ParseError {
            kind,
            span: token.span,
            line: self.end_line(token),
            near: self.near(token),
        };
        self.events.push(Raw::Error(error));
        self.skip_rest();
    }

    /// Reports `kind` at `span`, without `near`, on the line where the last token read ends, as
    /// PUC-Rio's `luaK_semerror` does, and stops.
    fn error_at(&mut self, kind: ParseErrorKind, span: Span) {
        if self.stops_quietly() {
            return;
        }
        let last = self.token_at(self.pos.saturating_sub(1));
        let error = ParseError {
            kind,
            span,
            line: self.end_line(last),
            near: None,
        };
        self.events.push(Raw::Error(error));
        self.skip_rest();
    }

    /// Returns whether an error found here goes unreported: once the parser has stopped, and
    /// when the current token is at or past the one where PUC-Rio's lexer would find the
    /// source's first lexical error, which it reports first. In that case it stops here.
    fn stops_quietly(&mut self) -> bool {
        if self.stopped {
            return true;
        }
        let past_lex_error = self.lex_stop.is_some_and(|stop| self.pos >= stop);
        if past_lex_error {
            self.skip_rest();
        }
        past_lex_error
    }

    /// Returns what a message says an error at `token` is near, as PUC-Rio's `txtToken` writes
    /// it: a string's text as its lexer keeps it, the end of the source, a byte that isn't
    /// printable as `<\N>`, with `N` in decimal, and any other token's own bytes. An unexpected
    /// zero byte gives none, because PUC-Rio reads it as the token 0, which its messages leave
    /// out.
    fn near(&self, token: Token) -> Option<Near> {
        let text = self.text(token);
        match token.kind {
            TokenKind::Eof => Some(Near::Eof),
            TokenKind::String => Some(Near::Text(string_as_read(text))),
            TokenKind::Unknown => match text.first().copied().unwrap_or(0) {
                0 => None,
                byte @ b' '..=b'~' => Some(Near::Text(vec![byte])),
                byte => Some(Near::Text(format!("<\\{byte}>").into_bytes())),
            },
            // Every other token, Lunaferra's own among them, is written as its bytes.
            _ => Some(Near::Text(text.to_vec())),
        }
    }

    /// Puts every token from the current one to the end of the source, apart from `Eof`, in an
    /// error node, and stops: from here on, the parser is at the end of the source and reports
    /// nothing more.
    fn skip_rest(&mut self) {
        let skipped = self.start();
        while self.pos < self.eof {
            self.events.push(Raw::Token(self.pos));
            self.pos = self.pos.saturating_add(1);
        }
        // The node is complete before the parser stops, so it marks the place even when it is
        // empty, at the end of the source.
        self.complete(skipped, NodeKind::Error);
        self.stopped = true;
    }

    // The events.

    /// Turns the parser's events into [`Event`]s, writing each node's `Start` where its first
    /// child, or the first node it precedes, starts.
    fn into_events(self) -> Vec<Event> {
        let Self {
            lexed,
            events,
            mut nodes,
            ..
        } = self;
        let tokens = lexed.tokens();
        let mut out = Vec::with_capacity(events.len());
        let mut kinds = Vec::new();
        for event in events {
            match event {
                Raw::Start(first) => {
                    // The node, then each node started around it, innermost first.
                    let mut next = Some(first);
                    while let Some(node) = next.and_then(|at| nodes.get_mut(at)) {
                        if node.written {
                            break;
                        }
                        node.written = true;
                        kinds.extend(node.kind);
                        next = node.parent;
                    }
                    out.extend(kinds.drain(..).rev().map(Event::Start));
                }
                Raw::Token(at) => {
                    let token = tokens[at];
                    out.push(Event::Token(token.kind, token.span));
                }
                Raw::Finish => out.push(Event::Finish),
                Raw::Error(error) => out.push(Event::Error(error)),
            }
        }
        out
    }
}
