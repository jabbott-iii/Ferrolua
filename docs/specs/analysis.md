# Subsystem: `ferrolua-analysis`

`ferrolua-analysis` builds the semantic model that the type checker, the linter, the SAST tool, and
the language server share. For each file, it resolves every name, builds each function's control
flow graph, and computes which definitions reach each use. For a project, it reads `.ferrolua.lua`
(ADR-0016, proposed: a data-only Lua file) and links files through `require` calls. It works on the
lossless tree, so files with syntax errors still get a model. It lands in M6.1–M6.3.

## Why one shared model

Every tool asks the same questions: what does this name refer to, can this line run, where did this
value come from. Answering them once keeps the tools consistent, so the linter and the type checker
never disagree about which variable a name means, and lets the language server reuse one model per
edit. The model is computed per file and cached by content, so an edit recomputes only the files it
changed and the files that `require` them.

## Names and scopes

Names resolve under the same rules as the compiler ([`compiler.md`](compiler.md#name-resolution)):
locals, upvalues, and globals, with Lua 5.5's `global` declarations and strict mode. Each name has
a symbol with its definition, its uses, and its assignments, which the linter's unused-variable
rules and the language server's rename use.

## Control flow and data flow

Each function gets a control flow graph: blocks of statements joined by edges for `if`, loops,
`break`, `goto`, `return`, and calls that can raise errors. From the graph, the analysis computes
reaching definitions, so each use of a local knows which assignments it may see, and liveness, so
the linter knows which assignments are never read. The type checker narrows types along the same
edges.

## Modules

A call `require("name")` with a literal string resolves through the `package.path` patterns in
`.ferrolua.lua`, or Lua's default `./?.lua;./?/init.lua`. The returned value of the required file
becomes the call's value. Calls with a computed name resolve to `any`.

## Invariants

1. Every name resolves to the same variable the compiler resolves it to.
2. Analysis never panics, on any file, valid or not, and finishes in time proportional to the file's
   size times its nesting depth.
3. Re-analyzing after an edit gives the model a fresh analysis of the edited project gives.

## Tests

- Invariant 1: a property test compares the analysis's resolution with the compiler's, on
  generated programs.
- Invariant 2: the `analysis` fuzz target, and hostile-input tests with deep nesting.
- Invariant 3: a property test applies generated edits and compares incremental and fresh results.
