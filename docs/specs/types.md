# Subsystem: `ferrolua-types`

`ferrolua-types` is Ferrolua's type checker. It checks the inline annotations of
[`language.md`](language.md#type-annotations-adr-0006) and the LuaCATS annotations in comments, infers
types for unannotated locals, and reports `T` diagnostics before a script runs (ADR-0006: types are
inline, checked before run, and only hints at run time). Typing is gradual: code without annotations
is treated as `any` and produces no type errors, so a project can add types one file at a time. It
lands in M6.4–M6.8.

## Why gradual typing

Most Lua code has no types, and much of it builds tables whose shape changes as the program runs.
A sound type system that rejected every program it couldn't prove would reject most existing Lua.
A gradual system checks what is annotated and trusts `any` at the boundary, the trade-off
TypeScript and Luau make. The cost is that a wrong value from untyped code can still reach typed
code at run time.

## Types

| Type | Meaning |
| :- | :- |
| `nil`, `boolean`, `number`, `integer`, `string`, `thread`, `userdata` | Lua's basic types; `integer` is a subtype of `number` |
| `true`, `false`, `"literal"` | Singleton types, for tags and options |
| `{T}` | An array of `T`, a table with keys 1 to n |
| `{[K]: V}` | A map from `K` to `V` |
| `{ name: T, ... }` | A record with named fields |
| `(A, B) -> R` | A function type |
| `A \| B`, `T?` | A union, and `T \| nil` |
| `any` | Unchecked: any value, and assignable to anything |
| `unknown` | Any value, but must be narrowed before use |
| `never` | No value, the type of a function that always raises |
| A class name | An instance of that class or its subclasses (ADR-0007) |
| `T<A>` | A generic alias or class applied to arguments |

Records are structural: a value with at least the fields of a record type is assignable to it.
Classes are nominal: only instances of a class or its subclasses have its type.

## Checking

- **Inference:** a local without an annotation takes the type of its first value, widened to the
  type of later assignments in the same scope.
- **Narrowing:** inside `if type(x) == "string" then`, `x` is a string; after `if x then`, `x` isn't
  `nil` or `false`; `assert(x)` narrows the same way.
- **Calls:** argument counts and types are checked against the function's type; in strict files,
  the arity rule of [`language.md`](language.md#arity-enforcement) applies statically too.
- **Tables:** a table constructor assigned to a record type must have its required fields.
- **Collections:** `list<T>`, `tuple<A, B>`, and `set<T>` type lists, tuples, and sets
  ([`language.md`](language.md#type-annotations-adr-0006)), and the checker infers them from
  literals, so `[1, 2]` is a `list<integer>`.

## Declarations

Types for the standard library, and for host APIs, come from declaration files: Ferrolua files
with `declare` statements and no code, such as `declare function print(...: any)`. Ferrolua ships
declarations for all of its standard library. Projects list their own in `declarations` in
`.ferrolua.lua` ([`diagnostics.md`](diagnostics.md#configuration-ferrolualua)). M6.6 settles the
declaration syntax. A host writes its declaration file from the same `Api` description that
registers its bindings, so the two never drift (ADR-0035: the host's API described once; M6.12).

## Which files are checked

A file is checked when it has at least one annotation, inline or LuaCATS, or starts with
`--!strict`, or when `.ferrolua.lua` sets `types = { check_all = true }`. Other files get no `T`
diagnostics, so adding the checker to an untyped project reports nothing until the project opts
in.

## LuaCATS annotations

The checker reads `---@type`, `---@param`, `---@return`, `---@class`, `---@field`, `---@alias`, and
`---@generic` comments, as lua-language-server documents them, and treats them like the inline
syntax. Where a file has both for the same name, the inline annotation wins, and the linter reports
the conflict.

## Invariants

1. A file that isn't checked, under the rule above, gets no `T` diagnostics.
2. Checking never panics and finishes on every input, including recursive and mutually recursive
   types.
3. Removing every annotation from a file that has no `T` errors leaves a file that still has none.
4. The standard library declarations accept every call that the conformance scripts make without
   an error.

## Tests

- Invariant 1: a test runs the checker on every conformance script, which has no annotations, and
  expects no `T` diagnostics.
- Invariant 2: the `types` fuzz target, and hostile-input tests with deeply nested and recursive
  types.
- Invariant 3: a property test on generated typed programs.
- Invariant 4: a test runs the checker on every conformance script with `check_all` on. The lines
  that pass wrong arguments on purpose carry a `--ferrolua:allow` comment for their `T` code, and
  the test expects no other `T` diagnostics.
- Each `T` code has a test file in `crates/types/tests/` with one case that triggers it and one
  near miss that doesn't.
