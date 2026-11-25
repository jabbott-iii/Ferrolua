# Subsystem: `lunaferra-syntax`

`lunaferra-syntax` turns Lunaferra source into tokens and trees. It has one parser that emits
events, and two builders that consume them: a lossless syntax tree for the tools and a compact tree
for the compiler (ADR-0009, proposed: one parser for the compiler and the tools). It also defines
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

Lua source is bytes, not necessarily UTF-8, so every position is a byte offset.

```rust
pub struct SourceId(u32);
pub struct Span { pub source: SourceId, pub start: u32, pub end: u32 }
pub struct LineMap { /* byte offsets of line starts */ }

impl LineMap {
    pub fn line_col(&self, offset: u32) -> (u32, u32);   // 1-based, columns in bytes
}
```

Lines end at `\n`, `\r`, `\r\n`, or `\n\r`, as PUC-Rio's lexer counts them. A source longer than
`u32::MAX` bytes is rejected before lexing.

## Lexer

The lexer reads Lua 5.5's tokens (§3.1 of the manual), apart from `..` and `~=`, and the new ones
of [`language.md`](language.md#tokens-and-keywords), such as `**`, `!=`, and `**=`:

- names, the 24 reserved words, which are Lua 5.5's 23 and `is`, and the contextual keywords of
  `language.md`;
- integers and floats in decimal and hexadecimal, including hexadecimal floats such as `0x1p4`,
  with the same overflow rules as PUC-Rio: a decimal integer that doesn't fit becomes a float;
- short strings with every escape, `\z`, `\xXX`, `\ddd`, and `\u{XXX}` up to `2^31`, and long
  strings and comments with any level of `=` signs;
- f-strings: an `f` touching a short or long string starts one (ADR-0025: f-strings, locals by
  default, lists, tuples, and sets, and Python's truth). The lexer splits it into text parts and
  fields, and lexes each field's expression into ordinary tokens, counting brackets and skipping
  strings to find the field's end, so the parser reads a field as an expression;
- a first line starting with `#`, which is skipped, as `lua` does for scripts that start with `#!`.

Each token keeps its leading and trailing trivia, the spaces, newlines, and comments around it, so
the lossless tree can reproduce the source exactly.

## Parser and events

The parser is hand-written, with recursive descent for statements and precedence climbing for
expressions, using the precedence table of [`language.md`](language.md#precedence). It emits
events:

```rust
pub enum Event {
    Start(NodeKind),
    Token(TokenKind, Span),
    Finish,
    Error(Diagnostic),
}
```

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

## Tests

- Invariants 1 and 2: the `lexer` and `parser` fuzz targets check the round trip and the absence of
  panics on generated and mutated input, and a hostile-input test checks time on inputs from 1 KB to
  16 MB, including 100,000 nested parentheses.
- Invariant 3: a property test on generated valid programs.
- Invariant 4: `tests/conformance/syntax_errors.lua` loads broken chunks with `load` and prints each
  message, recorded from its Lua 5.5 twin on PUC-Rio Lua 5.5.1.
- Invariant 5: hostile-input tests at the limit and one level past it, for blocks, parentheses,
  tables, and function bodies.
- Invariant 6: the precedence property test (M1.5).
