# Subsystem: `lunaferra-syntax`

`lunaferra-syntax` turns Lunaferra source into tokens and trees. It has one parser that emits
events, and two builders that consume them: a lossless syntax tree for the tools and a compact tree
for the compiler (ADR-0009: one parser for the compiler and the tools). It also defines
spans, line maps, and the `Diagnostic` type every tool reports ([`diagnostics.md`](diagnostics.md)).
It accepts the language of [`language.md`](language.md), Lua 5.5's syntax with Python's operators
(ADR-0023: a language of its own, with Python's operators and 0-based indexes), never panics on any
input, and enforces the size and nesting limits in [`sandbox.md`](sandbox.md). It lands in M1; the
syntax of compound assignment, types, classes, and strict mode lands in M4.

## Why one parser with two outputs

The compiler wants a fast parse of valid code and a small tree. The formatter needs every comment
and space; the language server needs a tree for code that is half typed. Two parsers would accept
different programs over time. One parser that reports what it sees as events lets each consumer
build only what it needs, and both see the same program. The cost is one layer of indirection
that a parser written only for the compiler wouldn't have.

## Source and spans

Lua source is bytes, not necessarily UTF-8, so every position is a byte offset, and every column
counts bytes. Built in M1.1 as described here:

```rust
pub const MAX_SOURCE_LEN: u32 = u32::MAX - 1;

pub struct SourceId(u32);   // SourceId::new(index) makes one, and id.index() reads it back
pub struct Span { pub source: SourceId, pub start: u32, pub end: u32 }   // end excluded
pub struct LineMap { /* byte offsets of line starts */ }

impl LineMap {
    pub fn new(source: &[u8]) -> Result<LineMap, Error>;   // Error::SourceTooLong
    pub fn line_col(&self, offset: u32) -> (u32, u32);    // 1-based, columns in bytes
}
```

Lines end at `\n`, `\r`, `\r\n`, or `\n\r`, as `inclinenumber` in PUC-Rio Lua 5.5.1's `llex.c`
counts them: it takes the other byte of a pair along when it comes next, so `\n\n` and `\r\r` end
two lines. A line break belongs to the line it ends, so the end of a source that ends with one is
on an empty last line. `line_col` gives an offset past the end of the source the position of the
end, so an offset from damaged input can't make it panic.

A source longer than `MAX_SOURCE_LEN` bytes is rejected before lexing: `LineMap::new` returns
`Error::SourceTooLong`. The limit is one byte below `u32::MAX`, so that every offset, the end of
the source included, and every 1-based line and column fit in a `u32`. In a source of `u32::MAX`
bytes with no line break, the column just past the last byte would be `2^32`. The source size
limit of [`sandbox.md`](sandbox.md#limits), 16 MB by default, is far lower; M1.8, the task that
adds the parser's size and nesting limits, will enforce it.

## Lexer

The lexer reads Lua 5.5's tokens (§3.1 of the manual), apart from `..` and `~=`, and the new ones
of [`language.md`](language.md#tokens-and-keywords), such as `**`, `!=`, and `**=`:

- names, the 24 reserved words, which are Lua 5.5's 23 and `is`, and the contextual keywords of
  `language.md`, which it reads as names;
- integers and floats in decimal and hexadecimal, including hexadecimal floats such as `0x1p4`,
  with the same overflow rules as PUC-Rio: a decimal integer that doesn't fit becomes a float;
- short strings with every escape, `\z`, `\xXX`, `\ddd`, and `\u{XXX}` up to `7FFFFFFF`, which is
  2^31 − 1, and long strings and comments with any level of `=` signs;
- f-strings: an `f` touching a short or long string starts one (ADR-0025: f-strings, locals by
  default, lists, tuples, and sets, and Python's truth). The lexer splits it into text parts and
  fields, and lexes each field's expression into ordinary tokens, counting brackets and skipping
  strings to find the field's end, so the parser reads a field as an expression;
- in a file, a UTF-8 byte order mark at the start, then a first line starting with `#`, which are
  skipped as PUC-Rio's `luaL_loadfilex` skips them for `lua`, `dofile`, `loadfile`, and
  `require`. A string that `load` reads skips neither, so there `#` is the length operator, as in
  PUC-Rio.

Each token keeps its leading and trailing trivia, the spaces, newlines, and comments around it, so
the lossless tree can reproduce the source exactly.

Built in M1.2 as described here:

```rust
pub enum Origin { File, String }   // a file skips its byte order mark and `#` line
pub fn lex(source: &[u8], id: SourceId, origin: Origin) -> Result<Lexed, Error>;   // SourceTooLong

pub struct Lexed { /* tokens, trivia, errors, and the line map */ }
impl Lexed {
    pub fn tokens(&self) -> &[Token];   // the last is always TokenKind::Eof
    pub fn trivia(&self) -> &[Trivia];
    pub fn leading(&self, token: &Token) -> &[Trivia];
    pub fn trailing(&self, token: &Token) -> &[Trivia];
    pub fn errors(&self) -> &[LexError];
    pub fn lines(&self) -> &LineMap;
}

pub struct Token { pub kind: TokenKind, pub span: Span, /* where its trivia is */ }
pub struct Trivia { pub kind: TriviaKind, pub span: Span }
pub enum TriviaKind { Whitespace, LineBreak, Comment, LongComment, Shebang, ByteOrderMark }

pub struct LexError { pub kind: LexErrorKind, pub span: Span, pub line: u32, pub near: Near }
pub enum Near { Text(Vec<u8>), Eof }
impl LexError {
    pub fn message(&self) -> Vec<u8>;   // such as `malformed number near '0x'`
}

pub enum Number { Integer(i64), Float(f64) }
pub fn numeral_value(text: &[u8]) -> Option<Number>;   // as `luaO_str2num` reads a numeral
```

### Tokens and trivia

Every byte belongs to exactly one token or one piece of trivia. A token's trailing trivia is the
spaces and comments after it on its line, up to the next line break; its leading trivia is the
rest of the trivia since the token before it, and the `Eof` token's leading trivia holds what
follows the last token. A long comment that starts on a token's line is trailing trivia, even when
it ends on a later line. Spaces inside an f-string's text are text, so the tokens there have no
trivia.

`TokenKind` has a kind for each reserved word, `Name`, `Integer`, `Float`, `String` for short and
long strings, the parts of an f-string, and each operator and symbol. Three more kinds carry what
the parser reports: `DotDot` and `TildeEqual` are Lua's `..` and `~=`, read so the parser can name
`+` and `!=` (M1.5); `Unknown` is one byte that starts no token, such as `@`, a lone `!`, or a byte
outside ASCII, which PUC-Rio's parser reports as an `unexpected symbol`; and `Error` holds bytes
the lexer couldn't read. The lexer takes the longest token it can, so `list<list<T>>` ends with
`>>`; the parser of type annotations (M4.3) splits it.

A numeral reaches as far as PUC-Rio's `read_numeral` reads: every hexadecimal digit and `.` that
follows, an exponent mark with its sign, and one letter or `_` touching the end, so `3x` and `1..2`
are each one malformed numeral, while `3-4` is three tokens. `numeral_value` then gives its value
as `luaO_str2num` does: an integer when it is one, with hexadecimal integers wrapping around
modulo 2^64, and otherwise a float if C's `strtod` reads all of it. Lunaferra converts decimal
floats with the standard library and hexadecimal floats with its own code, both rounding to the
nearest float as glibc's `strtod` does, so `1e309` is infinite and `0x1p-1075` is 0.

### f-strings as tokens

An f-string comes out as a run of tokens. For `f"a{x!r:>8}b"` they are `FStringStart` (`f"`),
`FStringText` (`a`), `FStringFieldStart` (`{`), `Name` (`x`), `FStringConversion` (`!r`),
`FStringSpec` (`:`), `FStringText` (`>8`), `FStringFieldEnd` (`}`), `FStringText` (`b`), and
`FStringEnd` (`"`). `FStringStart` of a long f-string takes the line break after the bracket, which
isn't part of the text, as in a long string. A spec may hold fields of its own, which come out the
same way, and a field may hold f-strings of its own. The lexer keeps the f-strings, fields, and
specs it is inside on a stack, not in recursive calls, so 100,000 nested f-strings can't overflow
the call stack. [`language.md`](language.md#formatted-string-literals-adr-0025) gives the rules
that decide where a field and its spec end.

### Lexical errors

Each lexical error has the words of PUC-Rio Lua 5.5.1's `llex.c`, such as
`unfinished long string (starting at line 3) near <eof>`, apart from the one for a single `}` in an
f-string, which `language.md` records. Each also has the line PUC-Rio's lexer is on when it finds
the mistake: for an unfinished string, the line where the string's line or the source ends.
The text after `near` is what PUC-Rio's buffer holds. For a numeral or a long bracket, that is its
bytes so far; for a short string, it is the opening quote and the bytes that each byte and escape
so far stands for, followed by the bytes of the escape that failed, so `"\65\q"` gives
`invalid escape sequence near '"A\q'`. As in PUC-Rio, that text stops at its first zero byte.

PUC-Rio stops at the first error. Lunaferra's lexer goes on, so the tools see the whole source: it
makes the bytes an `Error` token, reports the first error in each token and in each stretch of an
f-string's text, and, after an escape that fails, goes on from the byte that failed, so the quote
in `"\x"` still ends the string. Only the first error of a source is the one PUC-Rio reports.

Two of `llex.c`'s errors aren't reproduced, because each needs a source of gigabytes, which no test
can build. `chunk has too many lines` needs 2^31 − 1 lines, so a source of at least 2 GB.
`lexical element too long` needs a token of two thirds of PUC-Rio's `MAX_SIZE`: about 2.9 GB on a
32-bit target such as the web, and more than any source on a 64-bit one. Both are far past the
16 MB source limit of [`sandbox.md`](sandbox.md#limits), but within `Options::trusted()`, which
sets none; M1.8, the task that enforces the source limit, decides how to handle them.

## Parser and events

The parser is hand-written, with recursive descent for statements and precedence climbing for
expressions, using the precedence table of [`language.md`](language.md#precedence). It emits
events, which list the tree in the order a reader meets its parts: `Start` opens a node inside the
node that is open, `Token` adds the source's next token to the node that is open, and `Finish`
closes it.

```rust
pub enum Event {
    Start(NodeKind),
    Token(TokenKind, Span),
    Finish,
    Error(Diagnostic),
}

pub enum NodeKind { Chunk, Block, Error, /* one for each statement and expression */ }
```

`Event` and `NodeKind` were built in M1.3, with only the three kinds that frame a source: `Chunk`,
the root, which holds a `Block` and then the `Eof` token; `Block`; and `Error`, the tokens the
parser skipped to recover. The parser itself lands in M1.4, which adds a kind for each statement,
and in M1.5, which adds one for each expression. `Error(Diagnostic)` joins `Event` with the first
syntax error the parser reports, in M1.4, and carries the `Diagnostic` of M1.6.

List and tuple literals, and the fields of f-strings, parse as
[`language.md`](language.md#lists-tuples-and-sets-adr-0025) and
[`language.md`](language.md#formatted-string-literals-adr-0025) define them. After `(` and an
expression, a comma makes a tuple and `)` makes a parenthesized expression, and `()` is the empty
tuple. The parser decides after that expression, as the next paragraph describes.

A node can start before a part the parser has already read, so the parser can wrap a left
operand in a binary, chained, or conditional node once it sees the operator. That is how it reads
the condition after an `if` that follows an expression before it knows whether the `if` starts a
conditional expression or an `if` statement
([`language.md`](language.md#conditional-expressions)).

When the parser meets a token it doesn't expect, it emits an error, then skips to a token that can
start or end a statement, such as `end`, `local`, or a newline before a name, so the rest of the
file still parses. A depth counter stops nesting at PUC-Rio Lua 5.5.1's limit, with its message;
M1.8 records both here.

## Lossless tree

The lossless tree is built from the events. Every byte of the source belongs to exactly one token,
and an error node marks each place the parser recovered. Typed views over the generic nodes give
the tools named accessors, such as `IfStat::condition()`. The formatter, the analysis crates, and
the language server use it.

Built in M1.3 as described here:

```rust
impl SyntaxTree {
    pub fn build(source: &[u8], lexed: &Lexed, events: &[Event]) -> Result<SyntaxTree, Error>;
    pub fn root(&self) -> SyntaxNode<'_>;
    pub fn source(&self) -> &[u8];
}

impl<'t> SyntaxNode<'t> {     // a handle that borrows its tree, cheap to copy
    pub fn kind(&self) -> NodeKind;
    pub fn children(&self) -> Children<'t>;   // an iterator of SyntaxElement, in source order
    pub fn text(&self) -> Vec<u8>;            // its tokens, each with its trivia
}

impl<'t> SyntaxToken<'t> {
    pub fn kind(&self) -> TokenKind;
    pub fn span(&self) -> Span;
    pub fn text(&self) -> &'t [u8];
    pub fn leading(&self) -> &'t [Trivia];
    pub fn trailing(&self) -> &'t [Trivia];
}

pub enum SyntaxElement<'t> { Node(SyntaxNode<'t>), Token(SyntaxToken<'t>) }

// The typed views, in the module `ast`:
pub trait AstNode<'t>: Sized {
    fn cast(node: SyntaxNode<'t>) -> Option<Self>;   // None for a node of another kind
    fn syntax(&self) -> SyntaxNode<'t>;
}
pub struct Chunk<'t>;       // block() and eof()
pub struct Block<'t>;
pub struct ErrorNode<'t>;
```

`build` accepts a list of events only when it makes one tree over every token of the source: the
first event opens the root, every later one falls inside the root, each `Finish` closes the node
opened last that is still open, and the `Token` events name each token of the `Lexed` once, in
order, by kind and span, up to and including `Eof`. Otherwise it returns
`Error::EventOutsideRoot` or `Error::TokenOutOfOrder`, with the position of the event at fault,
or `Error::EventsEndEarly`. So every tree that `build` returns holds every token of its source,
and gives back the source byte for byte. Because a `Token` event must match its token exactly,
the parser can't yet split one token in two; the parser of type annotations will need that to
read the `>>` that ends `list<list<T>>` as two `>` (M4.3).

A node's text is the bytes of the tokens inside it, each with its leading and trailing trivia, so
the root's text is the whole source. The tree keeps its own copy of the source and the trivia, so
it outlives the buffers it was built from. It is stored as one flat list in which each node comes
first, then everything inside it, and records where its descendants end. Building, walking,
printing, and dropping a tree therefore never recurse, so 100,000 nested parentheses can't
overflow the call stack, and `children` steps over a child node's descendants in one step.

A view's accessors look among its node's children for each part, and give `None` for a part that
isn't there, which is how the tools read a source with syntax errors. Each task that teaches the
parser a statement or an expression adds its node kind and its view (M1.4, M1.5).

## Compact tree

The compact tree is built from the same events, holds only what the compiler needs, and is built
only when there are no errors. It records each node's line for debug information, and keeps type
annotations as compact type expressions, so the compiler can store JIT hints without the type
checker.

## Syntax errors

For `load` and the compiler, a syntax error produces PUC-Rio's format,
`<chunkname>:<line>: <message> near '<token>'`, such as `input:3: 'end' expected near <eof>`. An
error in syntax that Lunaferra shares with Lua 5.5 has the same message and token text as on PUC-Rio
Lua 5.5.1; the messages for Lua's removed operators and for Lunaferra's own syntax are recorded in
`language.md`. For the tools, each error is a
`P` diagnostic with a span and, where certain, a fix.

## Invariants

1. Printing the lossless tree's tokens and trivia gives back the source, byte for byte, for every
   input, valid or not.
2. The parser never panics, and finishes in time proportional to the input's length.
3. For valid input, the compact tree built from the events matches the lossless tree's structure.
4. For every syntax error in syntax Lunaferra shares with Lua 5.5, Lunaferra reports PUC-Rio Lua
   5.5.1's message and line.
5. Nesting one level past PUC-Rio Lua 5.5.1's limit stops with PUC-Rio's message.
6. Every expression groups as the precedence table says ([`language.md`](language.md#invariants),
   invariant 1).
7. The line map gives every byte offset the line that PUC-Rio Lua 5.5.1's lexer counts for it, for
   any bytes, valid UTF-8 or not, and a source longer than `MAX_SOURCE_LEN` bytes is rejected.
8. `numeral_value` gives every numeral the value PUC-Rio Lua 5.5.1 gives it, with each float
   rounded to the nearest float and a tie to even.
9. `SyntaxTree::build` accepts exactly the lists of events that make one tree over every token of
   the source, and walking the tree it builds gives back those events.

## Tests

- Invariants 1 and 2: the `lexer` and `parser` fuzz targets check the round trip and the absence of
  panics on generated and mutated input, and a hostile-input test checks time on inputs from 1 KB to
  16 MB, including 100,000 nested parentheses. Until those land (M1.8, M1.9),
  `crates/syntax/tests/lexer.rs` checks the lexer's half of invariant 1: a property test writes
  out the tokens and trivia of 5,000 generated sources, lexed both as a string and as a file, and
  compares them with the source, and other tests do the same for every conformance script and for
  100,000 nested f-strings. `crates/syntax/tests/tree.rs` checks the tree's half: a property test
  builds trees of random shape over the tokens of 2,000 generated sources and compares each root's
  text with its source, and other tests do the same for every conformance script and for a tree
  nested 100,000 levels deep.
- Invariant 3: a property test on generated valid programs.
- Invariant 4: `tests/conformance/syntax_errors.lua` loads broken chunks with `load` and prints each
  message, recorded from its Lua 5.5 twin on PUC-Rio Lua 5.5.1. For the lexer's errors,
  `crates/syntax/tests/lexer.rs` compares the first error of each of 54 broken chunks with the
  message that `load` gave for it on PUC-Rio Lua 5.5.1 on 2026-11-25.
- Invariant 5: hostile-input tests at the limit and one level past it, for blocks, parentheses,
  tables, and function bodies.
- Invariant 6: the precedence property test (M1.5).
- Invariant 7: `crates/syntax/tests/source.rs` checks the line and column of every offset of every
  source of up to seven bytes made from `a`, `\n`, `\r`, and 0xFF, a byte that never appears in
  valid UTF-8, against a model of `inclinenumber`. It also checks each line ending and a source of
  `u32::MAX` bytes, and unit tests in `crates/syntax/src/source.rs` check the length limit at
  `MAX_SOURCE_LEN` and one byte past it. `crates/syntax/tests/lexer.rs` checks the count after a
  file's skipped first line that holds a `\r`, as PUC-Rio gave it for such a file.
- Invariant 8: unit tests in `crates/syntax/src/lexer/numeral.rs` compare 19 hexadecimal floats
  near the limits and ties of rounding with the bits PUC-Rio Lua 5.5.1 gave them, read back 2,000
  floats written exactly in hexadecimal, and compare 2,000 more of 65 to 128 bits, half of them
  ties, with a model that rounds in a `u128`. `tests/conformance/lexer.lua` prints numerals of each
  form, recorded from its twin on PUC-Rio Lua 5.5.1, and runs once the conformance runner does
  (M2.14).
- Invariant 9: a property test in `crates/syntax/tests/tree.rs` gives `SyntaxTree::build` 3,000
  lists of events, many of them changed by one edit that may break the tree, such as an event
  removed or two swapped, and compares its answer with a second working-out of the rule. For each
  tree it builds, the test walks the tree and compares the events it gives with the list. Other
  tests in the file check each error on a list of its own.
