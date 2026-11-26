//! The parser: [`parse`] reads the tokens of a source and returns the [`Event`]s of its tree,
//! which [`SyntaxTree::build`](crate::SyntaxTree::build) turns into the lossless tree.
//!
//! It is hand-written, with recursive descent for statements and precedence climbing for
//! expressions. Where the syntax is Lua 5.5's, it follows PUC-Rio Lua 5.5.1's `lparser.c` rule by
//! rule, so it accepts the same statements and reports each mistake with the same message and
//! line; its expressions have Python's operators and precedence, as the language spec defines
//! them. The statements and the expressions are modules of their own, and this module holds what
//! both use: the cursor over the tokens, the markers that open and close nodes, the errors, and
//! the recovery after each error.
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
//!
//! # An `if` after an expression
//!
//! Lua needs nothing between two statements, so in `x = f if c then g() end`, an `if` statement
//! follows an assignment, while in `x = f if c else g`, the `if` is part of a conditional
//! expression. Both start with `if` and a condition after the expression, and only the token
//! after the condition tells them apart. So the parser reads the `if` and the condition as a
//! conditional expression's, and when no `else` follows and the statement may end after the
//! expression, it moves their events aside, out of the statement's nodes. The statement finishes
//! without them, and the `if` statement then starts with them, and goes on from the condition.
//! Each event moves once, so the parser never reads a token twice.
//!
//! # Recovery
//!
//! PUC-Rio stops at a source's first error, but the tools need the whole file, so the parser goes
//! on after each one. When it meets a token it doesn't expect, it reports the error, then puts
//! the tokens up to a *stop*, a token that can start or end a statement or close what is open,
//! in an error node. Then it *unwinds*: the cursor shows the end of the source, so each construct
//! that is open finishes as far as it got, without reporting more, until the parser reaches a
//! block's list of statements or an `expect` of the token at the stop. There it goes on as
//! before. The checks of a rule that a statement breaks, such as `break` outside a loop, report
//! their error and go on without skipping anything, because the tokens are all in place.
//!
//! Two rules keep one mistake from giving several errors. An error found where the last
//! recovery stopped, before any token is read, isn't reported, and neither is an error at a
//! token that holds a lexical error, which the lexer has reported already. A token the lexer
//! couldn't read, such as a malformed number, stands where it is as an operand.

mod error;
mod expression;
mod statement;

pub use error::{ParseError, ParseErrorKind};

use crate::lexer::string_as_read;
use crate::source_error::token_of;
use crate::{Event, LexErrorKind, Lexed, MAX_DEPTH, Near, NodeKind, Span, Token, TokenKind};

/// Reads the tokens of `source`, which `lexed` holds, and returns the events of its tree.
///
/// `lexed` must be what [`lex`](crate::lex) gave for `source`. With any other source, the error
/// messages quote the wrong text, though nothing panics.
///
/// Each syntax error comes as an [`Event::Error`], and the parser goes on after it, so the rest
/// of the source still parses. After a token it doesn't expect, it puts the tokens it skips in a
/// [`NodeKind::Error`] node, finishes each node it had started as far as it got, and goes on at
/// the next token that can start or end a statement, such as `end`, `local`, or a name at the
/// start of a line, or that closes a bracket opened before the error. It reports no error at a
/// token that holds a lexical error, which [`Lexed::errors`] holds, and none at the same token
/// twice. The events always make a tree over every token, which
/// [`SyntaxTree::build`](crate::SyntaxTree::build) accepts.
///
/// PUC-Rio Lua stops at a source's first error, so only the first error of a source, lexical or
/// syntactic, is the one it reports. [`source_errors`](crate::source_errors) puts the two kinds
/// in the order PUC-Rio would meet them.
///
/// # Examples
///
/// ```
/// use lunaferra_syntax::{Limits, Event, Origin, SourceId, SyntaxTree, lex, parse};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = b"local x = 1\nif x then print(x) end\n";
/// let lexed = lex(source, SourceId::new(0), Origin::String, &Limits::default())?;
/// let events = parse(source, &lexed);
/// let tree = SyntaxTree::build(source, &lexed, &events)?;
/// assert_eq!(tree.root().text(), source);
///
/// let broken = b"if x then\nprint(x\nend\ny = = 1\n";
/// let lexed = lex(broken, SourceId::new(1), Origin::String, &Limits::default())?;
/// let errors: Vec<String> = parse(broken, &lexed)
///     .iter()
///     .filter_map(|event| match event {
///         Event::Error(error) => Some(format!("{}: {error}", error.line)),
///         _ => None,
///     })
///     .collect();
/// assert_eq!(
///     errors,
///     [
///         "3: ')' expected (to close '(' at line 2) near 'end'",
///         "4: unexpected symbol near '='",
///     ]
/// );
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
    /// The positions of the tokens that hold a lexical error, in order: the first token that ends
    /// after each error starts, and the `Eof` token after an unfinished string or comment that
    /// reaches the end of the source. The parser reports no error at them.
    lexical: Vec<usize>,
    /// Whether the parser is unwinding after an error: the cursor shows the end of the source
    /// until a block's statements, or an `expect` of the token at the stop, go on.
    unwinding: bool,
    /// The position where the last recovery stopped, at which no error is reported again.
    recovered_at: Option<usize>,
    /// Whether the innermost construct the parser is in is a list between brackets, such as a
    /// table's fields or a call's arguments, rather than a block, which changes where recovery
    /// stops.
    in_brackets: bool,
    /// The events so far, in the parser's own form.
    events: Vec<Raw>,
    /// One entry for each node started, in the order started.
    nodes: Vec<Node>,
    /// One entry for each function the parser is inside, the innermost last. The first is the
    /// main chunk's.
    functions: Vec<Function>,
    /// The `if` statement that starts with the `if` and the condition read last after an
    /// expression, until the statement that holds the expression is finished.
    pending_if: Option<PendingIf>,
    /// How many levels of nesting the parser is inside, counted as [`MAX_DEPTH`] says.
    depth: u32,
    /// Whether the parser reported `C stack overflow` and hasn't started a statement below the
    /// limit since, so that one nesting too deep gets one error, not one for each statement or
    /// operand past the limit.
    too_deep: bool,
}

/// The start of an `if` statement that the parser read after an expression, as the start of a
/// conditional expression, before the token after the condition showed it to be an `if`
/// statement's.
#[derive(Debug)]
struct PendingIf {
    /// The events of the `if` token and the condition, moved out of the statement before it.
    events: Vec<Raw>,
    /// The line where the `if` ends.
    line: u32,
    /// The condition's node, or `None` after an error that leaves none.
    condition: Option<Done>,
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

/// When a node that holds nothing is a part that a broken statement lacks, and is left out of
/// the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lacking {
    /// While the parser is unwinding.
    IfUnwinding,
    /// Always: for a node started while the parser was unwinding, such as the block of an `if`
    /// whose `then` is missing, which the unwinding ended in.
    IfEmpty,
}

/// How far recovery skips before it looks for a stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Skip {
    /// To the first stop, which may be the current token.
    ToStop,
    /// Past the current token at least, because nothing open can go on from it, such as a
    /// token that starts no statement where one must start.
    AtLeastOne,
}

/// Returns whether `kind` ends a part of a construct, so that an `expect` of it ends the
/// unwinding after an error, when recovery stopped at a token of that kind.
const fn ends_unwinding(kind: TokenKind) -> bool {
    use TokenKind as T;
    matches!(
        kind,
        T::End
            | T::Until
            | T::Then
            | T::Do
            | T::Else
            | T::Semicolon
            | T::Comma
            | T::RightParen
            | T::RightBracket
            | T::RightBrace
            | T::FStringFieldEnd
            | T::FStringEnd
    )
}

/// Returns whether `kind` opens a bracket that a token of a kind that [`closes`] accepts
/// closes.
const fn opens(kind: TokenKind) -> bool {
    use TokenKind as T;
    matches!(
        kind,
        T::LeftParen | T::LeftBracket | T::LeftBrace | T::FStringStart | T::FStringFieldStart
    )
}

/// Returns whether `kind` closes a bracket that a token of a kind that [`opens`] accepts
/// opened.
const fn closes(kind: TokenKind) -> bool {
    use TokenKind as T;
    matches!(
        kind,
        T::RightParen | T::RightBracket | T::RightBrace | T::FStringEnd | T::FStringFieldEnd
    )
}

impl<'s> Parser<'s> {
    fn new(source: &'s [u8], lexed: &'s Lexed) -> Self {
        let tokens = lexed.tokens();
        let eof = tokens.len().saturating_sub(1);
        let mut lexical: Vec<usize> = lexed
            .errors()
            .iter()
            .map(|error| token_of(tokens, error))
            .collect();
        // What an unfinished string or comment at the end swallowed can't be found missing.
        let end = tokens.get(eof).map_or(0, |token| token.span.start);
        let swallows_rest = lexed.errors().iter().any(|error| {
            let unfinished = matches!(
                error.kind,
                LexErrorKind::UnfinishedString
                    | LexErrorKind::UnfinishedLongString { .. }
                    | LexErrorKind::UnfinishedLongComment { .. }
            );
            unfinished && error.span.end >= end
        });
        if swallows_rest {
            lexical.push(eof);
        }
        lexical.sort_unstable();
        lexical.dedup();
        Self {
            source,
            lexed,
            pos: 0,
            eof,
            lexical,
            unwinding: false,
            recovered_at: None,
            in_brackets: false,
            events: Vec::with_capacity(tokens.len().saturating_mul(2)),
            nodes: Vec::with_capacity(tokens.len()),
            functions: vec![Function {
                vararg: true,
                loops: 0,
            }],
            pending_if: None,
            depth: 0,
            too_deep: false,
        }
    }

    /// Reads the whole source: a chunk, which holds a block and the end of the source.
    ///
    /// The block's statements end at a token that ends a block, or after a `return`, as in
    /// PUC-Rio's `statlist`. Anywhere but at the end of the source, that is the error
    /// `<eof> expected`, after which the block goes on past the token.
    fn chunk(&mut self) {
        let chunk = self.start();
        let block = self.start();
        loop {
            self.statements();
            // `statements` returns after a `return` even while unwinding, and nothing encloses
            // the chunk to go on from.
            self.unwinding = false;
            if self.at(TokenKind::Eof) {
                break;
            }
            self.error_skipping(ParseErrorKind::Expected(TokenKind::Eof));
        }
        self.complete(block, NodeKind::Block);
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

    /// Returns the current token's kind, or [`TokenKind::Eof`] while the parser is unwinding,
    /// so that each construct that is open finishes as it would at the end of the source.
    fn current(&self) -> TokenKind {
        if self.unwinding {
            TokenKind::Eof
        } else {
            self.token().kind
        }
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
    ///
    /// While the parser is unwinding, a token that ends a part of a construct, such as `then`,
    /// `end`, or `)`, ends the unwinding when it is the token at the stop and the one asked for
    /// here, so the construct goes on from it.
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.unwinding {
            if !ends_unwinding(kind) || self.token().kind != kind {
                return false;
            }
            self.unwinding = false;
        } else if self.current() != kind {
            return false;
        }
        self.bump();
        true
    }

    /// Adds the current token and moves on if it is of `kind`, and otherwise reports that a
    /// token of `kind` was expected, as PUC-Rio's `checknext` does. After the error, it adds the
    /// token where the recovery stopped if that token is of `kind`.
    fn expect(&mut self, kind: TokenKind) {
        if !self.eat(kind) {
            self.error(ParseErrorKind::Expected(kind));
            self.eat(kind);
        }
    }

    /// Adds the current token and moves on if it is of `kind`, which closes the construct that
    /// `opener` opened on `line`, and otherwise reports it missing, as PUC-Rio's `check_match`
    /// does: naming the opener only when it is on another line. After the error, it adds the
    /// token where the recovery stopped if that token is of `kind`.
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
        self.eat(kind);
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
    /// While the parser is unwinding, a node that holds nothing is left out of the tree, so that
    /// the parts a broken statement lacks are missing, not empty.
    fn complete(&mut self, marker: Marker, kind: NodeKind) -> Done {
        self.complete_as(marker, kind, Lacking::IfUnwinding)
    }

    /// Completes the node `marker` started, as a node of `kind`, leaving it out of the tree if
    /// it holds nothing and `lacking` says the statement lacks it then.
    fn complete_as(&mut self, marker: Marker, kind: NodeKind, lacking: Lacking) -> Done {
        let done = Done {
            node: marker.0,
            kind,
        };
        let lacking = match lacking {
            Lacking::IfUnwinding => self.unwinding,
            Lacking::IfEmpty => true,
        };
        // Every marker comes from `start`, which pushed its node.
        let node = &mut self.nodes[marker.0];
        let empty = matches!(self.events.last(), Some(&Raw::Start(last)) if last == marker.0);
        if lacking && empty && !node.wraps {
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
    /// `luaX_syntaxerror` does, and recovers.
    fn error(&mut self, kind: ParseErrorKind) {
        let near = self.near(self.token());
        self.error_with(kind, near, Skip::ToStop);
    }

    /// Reports `kind` at the current token, without `near`, on the line where the token ends, and
    /// recovers. The messages for Lua's operators take this form, because each names its token.
    fn error_without_near(&mut self, kind: ParseErrorKind) {
        self.error_with(kind, None, Skip::ToStop);
    }

    /// Reports `kind` near the current token, as [`Parser::error`] does, at a token that nothing
    /// open can go on from, so the recovery skips at least that token.
    fn error_skipping(&mut self, kind: ParseErrorKind) {
        let near = self.near(self.token());
        self.error_with(kind, near, Skip::AtLeastOne);
    }

    /// Reports `kind` at the current token, on the line where it ends, with `near` after the
    /// message, then recovers, skipping as `skip` says, and returns whether it reported the
    /// error.
    ///
    /// Nothing is reported while the parser is unwinding, where the last recovery stopped, or at
    /// a token that holds a lexical error. The parser still recovers from the last two, because
    /// the token it found there is still wrong.
    fn error_with(&mut self, kind: ParseErrorKind, near: Option<Near>, skip: Skip) -> bool {
        if self.unwinding {
            return false;
        }
        let quiet = self.recovered_at == Some(self.pos) || self.holds_lexical_error(self.pos);
        if !quiet {
            let token = self.token();
            self.events.push(Raw::Error(ParseError {
                kind,
                span: token.span,
                line: self.end_line(token),
                near,
            }));
        }
        self.recover(skip, !quiet);
        !quiet
    }

    /// Reports `kind` near the current token, on the line where it ends, without recovering:
    /// for a rule that a statement breaks while its tokens are all in place, such as `break`
    /// outside a loop. Where the last recovery stopped, that token has an error already, so
    /// nothing is reported.
    fn report(&mut self, kind: ParseErrorKind) {
        if self.unwinding || self.recovered_at == Some(self.pos) {
            return;
        }
        let token = self.token();
        let near = self.near(token);
        self.events.push(Raw::Error(ParseError {
            kind,
            span: token.span,
            line: self.end_line(token),
            near,
        }));
    }

    /// Reports `kind` at `span`, without `near`, on the line where the last token read ends, as
    /// PUC-Rio's `luaK_semerror` does, without recovering: the checks of attributes and of
    /// `<close>` variables come after their tokens are read.
    fn error_at(&mut self, kind: ParseErrorKind, span: Span) {
        if self.unwinding {
            return;
        }
        let last = self.token_at(self.pos.saturating_sub(1));
        self.events.push(Raw::Error(ParseError {
            kind,
            span,
            line: self.end_line(last),
            near: None,
        }));
    }

    /// Returns whether the token at `pos` holds a lexical error, which the lexer has reported.
    fn holds_lexical_error(&self, pos: usize) -> bool {
        self.lexical.binary_search(&pos).is_ok()
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

    // Nesting.

    /// Counts one more level of nesting, as PUC-Rio's `enterlevel` does, and returns whether the
    /// parser may read it.
    ///
    /// One level past [`MAX_DEPTH`] is `C stack overflow` at the current token, which isn't
    /// counted: the recovery skips that token at least, so the parser always moves on, and
    /// unwinds to a shallower level. The caller reads nothing more there. Until the parser starts
    /// a statement below the limit, each later try is refused the same way without another error,
    /// so the statements of a 198th block, or the elements of a list that recovery goes on in,
    /// add none.
    fn enter(&mut self) -> bool {
        if self.depth < MAX_DEPTH {
            self.depth += 1;
            return true;
        }
        if !self.too_deep {
            self.too_deep = self.error_with(ParseErrorKind::NestingTooDeep, None, Skip::AtLeastOne);
        } else if !self.unwinding {
            self.recover(Skip::AtLeastOne, false);
        }
        false
    }

    /// Leaves `levels` levels of nesting that [`Parser::enter`] counted.
    fn leave(&mut self, levels: u32) {
        self.depth = self.depth.saturating_sub(levels);
    }

    // Recovery.

    /// Puts the tokens from the current one up to the next stop in an error node, then unwinds.
    ///
    /// After a reported error, the node marks the place even when it holds nothing; after one
    /// that wasn't reported, it is left out unless it holds a token.
    fn recover(&mut self, skip: Skip, reported: bool) {
        let stop = self.next_stop(skip);
        if reported || stop > self.pos {
            let skipped = self.start();
            while self.pos < stop {
                self.events.push(Raw::Token(self.pos));
                self.pos = self.pos.saturating_add(1);
            }
            self.complete(skipped, NodeKind::Error);
        }
        self.recovered_at = Some(self.pos);
        self.unwinding = true;
    }

    /// Returns the position of the first stop from the current token on, or from the token after
    /// it if `skip` says to skip at least one: a token that can start or end a statement, or a
    /// closing bracket whose opening one isn't among the skipped tokens. Inside a bracket that
    /// the skipped tokens open, only the tokens that close it count. The `Eof` token always
    /// stops.
    fn next_stop(&self, skip: Skip) -> usize {
        let mut at = self.pos;
        let mut depth = 0_u32;
        if skip == Skip::AtLeastOne && at < self.eof {
            if opens(self.token_at(at).kind) {
                depth = 1;
            }
            at = at.saturating_add(1);
        }
        while at < self.eof {
            let kind = self.token_at(at).kind;
            if opens(kind) {
                depth = depth.saturating_add(1);
            } else if closes(kind) {
                if depth == 0 {
                    break;
                }
                depth = depth.saturating_sub(1);
            } else if depth == 0 && self.stops_at(at) {
                break;
            }
            at = at.saturating_add(1);
        }
        at
    }

    /// Returns whether the token at `pos` can start or end a statement or an element, so
    /// recovery stops at it: a reserved word that only a statement or a block's end holds,
    /// `then`, `do`, `;`, or `::`, anywhere. In a block, a name, `function`, or `if` at the start
    /// of a line stops it too, while each of those also appears inside expressions. In a list
    /// between brackets, where such a name may start the next line's element, a `,` stops it
    /// instead.
    fn stops_at(&self, pos: usize) -> bool {
        use TokenKind as T;
        match self.token_at(pos).kind {
            T::End
            | T::Else
            | T::Elseif
            | T::Until
            | T::Then
            | T::Do
            | T::Semicolon
            | T::ColonColon
            | T::Local
            | T::Global
            | T::Return
            | T::Break
            | T::Goto
            | T::While
            | T::For
            | T::Repeat => true,
            T::Comma => self.in_brackets,
            T::Name | T::Function | T::If => !self.in_brackets && self.starts_line(pos),
            _ => false,
        }
    }

    /// Calls `read` to read the elements of a list between brackets, where recovery stops at a
    /// `,`, and returns what it returns.
    fn bracketed<R>(&mut self, read: impl FnOnce(&mut Self) -> R) -> R {
        let outer = std::mem::replace(&mut self.in_brackets, true);
        let result = read(self);
        self.in_brackets = outer;
        result
    }

    /// Returns whether the token at `pos` starts a line: it is the source's first token, or it
    /// starts on a later line than the line where the token before it ends.
    fn starts_line(&self, pos: usize) -> bool {
        let line = self.lexed.lines().line_col(self.token_at(pos).span.start).0;
        pos.checked_sub(1)
            .is_none_or(|before| line > self.end_line(self.token_at(before)))
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
