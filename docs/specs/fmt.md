# Subsystem: `lunaferra-fmt`

`lunaferra-fmt` formats Lua and Lunaferra source into one consistent layout. It works on the
lossless tree, keeps every comment, never changes what a program does, and gives the same output
when run twice. `lunaferra fmt` rewrites files, and `lunaferra fmt --check` reports `F` diagnostics
for files that aren't formatted, so CI can enforce the style. It lands in M7.4–M7.5.

## Why a formatter with few options

A formatter ends arguments about layout by making one choice for everyone, and each option
reopens an argument. Lunaferra's formatter has only the options that projects genuinely differ on:
indentation, line width, and quote style. Everything else, such as spacing around operators and
where long calls break, follows one style.

## Layout

The formatter builds a document from the tree: text, line breaks that may become spaces, and
groups that break together. It then fits the document to the line width with the algorithm from
Philip Wadler's "A prettier printer": a group stays on one line when it fits, and breaks every
optional line break in it when it doesn't.

- Comments stay attached to the token they follow or precede, on the same line or their own.
- Blank lines between statements are kept, up to one.
- Long strings and long comments are copied exactly, because their contents are data, and so is
  the text between an f-string's fields.
- A list literal whose first element is a list gets a space after its `[`, as `[ [1, 2] ]`,
  because `[[` would start a long string (ADR-0025: lists, tuples, and sets).
- Parentheses that Lua needs are kept, and ones it doesn't need are kept too, because
  `(f())` truncates `f`'s results to one value.

## Options

| Setting in the manifest's `tools` table | Values | Default |
| :- | :- | :- |
| `format.indent_style` | `"spaces"`, `"tabs"` | `"spaces"` |
| `format.indent_width` | 1 to 16 | 2 |
| `format.line_width` | 40 to 400 | 100 |
| `format.quote_style` | `"double"`, `"single"`, `"keep"` | `"double"` |

A string is requoted only when that needs no new escapes.

## Invariants

1. **Meaning:** a formatted file compiles to the same bytecode as the original, apart from line
   numbers in debug information.
2. **Idempotence:** formatting a formatted file changes nothing.
3. **Comments:** every comment in the input appears in the output, in the same order.
4. A file with syntax errors is left unchanged, with a `P` diagnostic.

## Tests

- Invariants 1–3: property tests on generated programs with comments placed between random tokens.
- Invariant 4: tests on broken files.
- Expected-output tests in `crates/fmt/tests/cases/`, one input and one output file per layout
  decision; changing one means reading the difference.
