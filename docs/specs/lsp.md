# Subsystem: `lunaferra-lsp`

`lunaferra-lsp` is Lunaferra's language server. It speaks the Language Server Protocol (LSP) 3.17
over standard input and output, so any editor with an LSP client gets auto complete, diagnostics,
hover, navigation, renaming, and formatting for Lua and Lunaferra files (ADR-0015, proposed). It
builds on the shared model of [`analysis.md`](analysis.md) and the types of [`types.md`](types.md),
so completion knows the fields of typed tables, the members of classes, the exports of required
modules, and the standard library. `lunaferra lsp` starts it. It lands in M9.

## Why completion is built on types

"Strong" auto complete means suggesting only what is valid at the cursor: after `player.`, the
fields a `player` has, not every name in the file. That needs the type of `player`, which the type
checker infers even when nothing is annotated, from the table constructor or class it came from.
Code being edited usually doesn't parse, so completion works on the lossless tree with its error
nodes ([`syntax.md`](syntax.md#lossless-tree)), and treats the token at the cursor as unfinished.

## Features

| Feature | LSP request | What it uses |
| :- | :- | :- |
| Diagnostics as you type | `textDocument/publishDiagnostics` | Syntax, strict mode, types, lint, SAST, and the `M` diagnostics of a package's manifest |
| Completion | `textDocument/completion` | Scopes, types, class members, module exports, the module names a package can require (ADR-0038, proposed: packages and modules), standard library declarations, keywords, and snippets for `function`, `for`, and `class` |
| Hover | `textDocument/hover` | Types and the doc comment of the name |
| Signature help | `textDocument/signatureHelp` | Parameter names, types, and which are optional |
| Go to definition and find references | `textDocument/definition`, `textDocument/references` | Symbols from the analysis |
| Rename | `textDocument/rename` | Symbols, across files for globals and module fields |
| Document symbols | `textDocument/documentSymbol` | Functions, classes, and top-level locals |
| Formatting | `textDocument/formatting` | [`fmt.md`](fmt.md) |
| Code actions | `textDocument/codeAction` | The fixes diagnostics carry |

## Documents and updates

The server keeps each open document's text and applies incremental edits. After an edit, it
re-parses that file, re-analyzes it and the files that depend on it, and publishes new
diagnostics. A request for an older version of a document than the server holds is answered with
the newest one.

## Invariants

1. The server never exits or panics on any message, valid or not, and answers every request it
   receives, with a result or an error.
2. Completion at any position in any file, parsed or not, returns within the budget in
   [`../00-overview.md`](../00-overview.md#budgets).
3. Every completion item is valid at the cursor: inserting it makes the file no worse to parse.
4. Rename changes exactly the references find-references returns.

## Tests

- Invariant 1: the `lsp` fuzz target sends generated message sequences, and hostile-input tests
  send truncated, oversized, and out-of-order messages.
- Invariant 2: a benchmark in `crates/lsp/benches/` that M9.6 makes a failing check.
- Invariant 3: a property test inserts each item at generated cursor positions.
- Invariant 4: a property test on generated projects.
- End-to-end tests start `lunaferra lsp` as a process and drive it through its standard input and
  output, as an editor would.
