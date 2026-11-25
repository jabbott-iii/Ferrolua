# Subsystem: `ferrolua-syntax`

`ferrolua-syntax` turns Lua source into tokens and trees. It has one parser that emits events, and
two builders that consume them: a lossless syntax tree for the tools and a compact tree for the
compiler (ADR-0009, proposed: one parser for the compiler and the tools). It also defines spans,
line maps, and the `Diagnostic` type every tool reports ([`diagnostics.md`](diagnostics.md)). It
accepts all of Lua 5.5 and Ferrolua's extensions ([`language.md`](language.md)), never panics on
any input, and enforces the size and nesting limits in [`sandbox.md`](sandbox.md). It lands in M1;
the extension syntax lands in M4.

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

The lexer reads Lua 5.5's tokens (§3.1 of the manual) and Ferrolua's new ones:

- names, the 23 reserved words of Lua 5.5, and the contextual keywords of `language.md`;
- integers and floats in decimal and hexadecimal, including hexadecimal floats such as `0x1p4`,
  with the same overflow rules as PUC-Rio: a decimal integer that doesn't fit becomes a float;
- short strings with every escape, `\z`, `\xXX`, `\ddd`, and `\u{XXX}` up to `2^31`, and long
  strings and comments with any level of `=` signs;
- a first line starting with `#`, which is skipped, as `lua` does for scripts that start with `#!`.

Each token keeps its leading and trailing trivia, the spaces, newlines, and comments around it, so
the lossless tree can reproduce the source exactly.

## Parser and events

The parser is hand-written, with recursive descent for statements and precedence climbing for
expressions, using the precedence table in §3.4.8 of the manual. It emits events:

```rust
pub enum Event {
    Start(NodeKind),
    Token(TokenKind, Span),
    Finish,
    Error(Diagnostic),
}
```

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
`<chunkname>:<line>: <message> near '<token>'`, such as `input:3: 'end' expected near <eof>`, with
the same messages and the same token text as PUC-Rio Lua 5.5.1. For the tools, each error is a
`P` diagnostic with a span and, where certain, a fix.

## Invariants

1. Printing the lossless tree's tokens and trivia gives back the source, byte for byte, for every
   input, valid or not.
2. The parser never panics, and finishes in time proportional to the input's length.
3. For valid input, the compact tree built from the events matches the lossless tree's structure.
4. For every syntax error PUC-Rio Lua 5.5.1 reports, Ferrolua reports the same message and line.
5. Nesting one level past PUC-Rio Lua 5.5.1's limit stops with PUC-Rio's message.
6. Every Lua 5.5 program tokenizes the same as under the Lua 5.5 grammar alone ([`language.md`](language.md), invariant 1).

## Tests

- Invariants 1 and 2: the `lexer` and `parser` fuzz targets check the round trip and the absence of
  panics on generated and mutated input, and a hostile-input test checks time on inputs from 1 KB to
  16 MB, including 100,000 nested parentheses.
- Invariant 3: a property test on generated valid programs.
- Invariant 4: `tests/conformance/syntax_errors.lua` loads broken chunks with `load` and prints each
  message, recorded from PUC-Rio Lua 5.5.1.
- Invariant 5: hostile-input tests at the limit and one level past it, for blocks, parentheses,
  tables, and function bodies.
- Invariant 6: the superset property test (M4.1).
