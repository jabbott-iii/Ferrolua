# Subsystem: `lunaferra-json`

`lunaferra-json` reads and writes JSON, as RFC 8259 defines it, and frames the messages of the
Language Server Protocol and the Debug Adapter Protocol. The command line uses it for
`--format json` and SARIF output, and the language and debug servers for every message they send
and receive. It exists because ADR-0002 rules out JSON crates, and ADR-0015 (proposed: editor
support through servers of Lunaferra's own) needs JSON. It lands in M6.9; message framing lands in
M9.1.

## Why a small JSON crate of Lunaferra's own

The tools need a JSON value type, a parser that rejects bad input with a clear position, and a
writer that escapes correctly. That is a few hundred lines, with a well-defined specification and a
clear test, so it is a good candidate to write rather than adopt. Messages from an editor are
outside input, so the parser has the same limits as any other.

## Values

```rust
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),         // integers up to 2^53 are exact
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),   // keeps the order keys arrived in
}
```

Objects keep their keys in order, so output is stable. A duplicate key is an error.

## Parser and writer

- The parser accepts exactly RFC 8259: no comments, no trailing commas, no leading zeros, no `NaN`.
  It checks that strings are valid UTF-8 and that `\u` escapes pair surrogates correctly.
- Nesting stops at 128 levels and a message at 64 MB by default, with an error that gives the byte
  offset.
- The writer escapes control characters, `"`, and `\`, and writes numbers in the shortest form that
  reads back the same.

## Message framing

Both protocols send each message as a header, `Content-Length: <bytes>\r\n\r\n`, followed by the
JSON body. The framing module reads a header, checks the length against the message limit, and
reads exactly that many bytes. A JSON-RPC 2.0 layer on top turns messages into requests,
responses, and notifications, matched by `id`.

## Invariants

1. Writing any `Json` and parsing the result gives the same value.
2. The parser accepts every valid document and rejects every invalid one, with an error and never a
   panic.
3. No message larger than the limit is read into memory.

## Tests

- Invariant 1: a property test on generated values, including every escape and surrogate pair.
- Invariant 2: test files of valid and invalid documents written for each rule of RFC 8259, and the
  `json` fuzz target.
- Invariant 3: hostile-input tests with lengths of 0, the limit, one past it, and a header that
  lies about its length.
