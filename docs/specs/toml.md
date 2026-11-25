# Subsystem: `lunaferra-toml`

`lunaferra-toml` reads, edits, and writes TOML, as [TOML v1.0.0](https://toml.io/en/v1.0.0)
defines it. TOML (Tom's Obvious, Minimal Language) is the format Cargo uses for `Cargo.toml`,
`Cargo.lock`, and `.cargo/config.toml`, and Lunaferra uses it the same way, for the package
manifest `Lunaferra.toml`, the lock file `Lunaferra.lock`, and the `config.toml` files of the
package commands ([`package.md`](package.md)). It exists because ADR-0002 (no third-party code)
rules out TOML crates, and ADR-0038 (packages and modules, as Cargo has, proposed) chose TOML. It
lands in M14.9.

## Why a TOML crate of Lunaferra's own

The package crate needs three things from TOML: values with the place each one came from, so a
diagnostic can point at the line a mistake is on; a parser that rejects bad input, since
manifests come from other people; and a document that keeps comments and layout, so
`lunaferra add` can change one line of a manifest the way `cargo add` does. TOML 1.0.0 has a
complete specification with examples for each rule, so the parser has a clear test. It is larger
than the JSON crate ([`json.md`](json.md)), because TOML has four kinds of string, dotted keys,
table headers, arrays of tables, and four kinds of date and time.

## Values

```rust
pub enum Toml {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Datetime(Datetime),   // an offset date-time, a local date-time, a local date, or a local time
    Array(Vec<Item>),
    Table(Table),
}

pub struct Item { pub value: Toml, pub span: Range<usize> }   // the bytes it came from
pub struct Table { /* keys in the order they appear, each with its item and the key's span */ }
```

Tables keep their keys in order, and every value keeps the byte range it was parsed from, so the
package crate can report an `M` diagnostic at the value that is wrong
([`diagnostics.md`](diagnostics.md#codes)).

## The parser

- It accepts exactly TOML 1.0.0: bare, quoted, and dotted keys; basic, literal, and multi-line
  strings with every escape; integers in decimal, hexadecimal, octal, and binary, with `_`
  between digits; floats, including `inf` and `nan`; booleans; the four kinds of date and time;
  arrays; inline tables; table headers; and arrays of tables.
- It rejects what TOML 1.0.0 forbids, with the line, column, and byte offset of the problem: a key
  or a table defined twice, a key added to an inline table after it closes, an integer outside
  64 bits, a leading zero, a control character in a string, and text that isn't UTF-8.
- Nesting of arrays and inline tables stops at 128 levels by default, and the caller sets the
  largest document it accepts, so deep or large input is an error and never a stack overflow or
  an allocation the caller didn't allow.

## The document

`Document` keeps every token of a file: comments, blank lines, whitespace, the order of keys, and
the form each value was written in, such as `0x1F` or a literal string. A tool can change one
value, add a key, or remove one, and every other byte of the file stays as it was. `lunaferra add`
and `lunaferra remove` edit manifests through it.

## The writer

The writer turns a `Table` into a file in one fixed layout: plain keys before subtables, each
table under its header, arrays of tables as `[[name]]` sections, strings in double quotes with
the escapes TOML requires, and floats in the shortest form that reads back the same. The lock
file is written this way, so the same value gives the same bytes on every platform.

## Invariants

1. Writing any `Table` and parsing the result gives the same table.
2. The parser accepts every valid TOML 1.0.0 document and rejects every invalid one, with an error
   that gives its position, and never panics.
3. Printing a `Document` that wasn't edited gives back the source, byte for byte.
4. An edit to one key changes only that key's line and the lines the edit adds or removes.
5. Nesting one level past the limit, or a document one byte past the caller's limit, is an error.

## Tests

- Invariant 1: a property test on generated tables, with every escape, each integer base, `inf`
  and `nan`, each kind of date and time, and arrays of tables.
- Invariant 2: test files of valid and invalid documents written here for each rule of TOML 1.0.0,
  and the `toml` fuzz target.
- Invariants 3 and 4: property tests parse generated documents with comments and every value form,
  print them, and compare the bytes; then apply generated edits and compare each changed line.
- Invariant 5: hostile-input tests with arrays and inline tables nested to the limit and one past
  it, and documents at the size limit and one past it.
