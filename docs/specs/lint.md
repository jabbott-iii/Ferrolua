# Subsystem: `ferrolua-lint`

`ferrolua-lint` is Ferrolua's linter: rules that find likely bugs, slow code, and code that keeps
memory alive by accident, in Ferrolua code. Each rule has an `L` code, a default
severity, and a test, and reads the shared model of [`analysis.md`](analysis.md) and the types of
[`types.md`](types.md). Users turn rules off or change their severity in `.ferrolua.lua`, or
suppress one line with a comment ([`diagnostics.md`](diagnostics.md)). It lands in M7.1–M7.3.

## Why a linter separate from the type checker

A type error means a program is wrong for every input. A lint finding means it is probably wrong,
or slower than it needs to be, and a project may reasonably disagree. Keeping lints separate lets
each project choose its rules without weakening the type checker, and lets rules use the types
when they exist and still work when they don't.

## Rules

Each rule's code is assigned when its task adds it to the registry in
[`diagnostics.md`](diagnostics.md#registry). The first set:

| Group | Rules |
| :- | :- |
| Correctness | Unused local, parameter, or loop variable; assignment that is never read; shadowing a local in the same function; unreachable code; read of an undeclared global in non-strict code; duplicate key in a table constructor; wrong argument count at a call to a known local function; comparison of a value with itself; an unused suppression comment |
| Porting | Habits from Lua that Ferrolua's operators and indexes turn into bugs (ADR-0023: Python's operators and 0-based indexes): `^` with a literal `2` or `3` on its right, which Lua code meant as a power and Ferrolua computes as XOR; a numeric `for` that starts at 1 and indexes a table with its control variable, which skips the element at 0; `#t + 1` used as a position to append at, one past the end |
| Memory | `Class.__index = Class` and other tables stored in themselves; parent and child tables that refer to each other; a coroutine stored where its own function can reach it; local functions that call each other, which form a loop. Only the loop collector frees these, later than counting would (ADR-0018) |
| Performance | String concatenation in a loop, where `table.concat` is linear; `table.insert(t, #t, v)`, where `t[#t] = v` is faster; a closure created in a hot loop that captures nothing that changes; a global read in a loop that could be a local |
| Style | Mixed LuaCATS and inline annotations for one name; an annotation that repeats the inferred type |

The memory rules find only the patterns they can see in one project. A loop built at run time from
data stays invisible to them; the loop collector frees those too
([`memory.md`](memory.md#the-loop-collector)).

## Configuration

```lua
return {
  rules = {
    L0001 = "off",      -- or "warning", or "error"
  },
}
```

## Invariants

1. Every rule has a test that triggers it and a near miss that doesn't.
2. A fix a rule offers, applied, removes that finding and changes no other behavior: the fixed file
   compiles to the same bytecode, apart from the fixed construct.
3. The linter never panics, on any file.

## Tests

- Invariant 1: one test file per rule in `crates/lint/tests/`.
- Invariant 2: a property test applies every offered fix to generated programs and compares the
  bytecode before and after.
- Invariant 3: the `lint` fuzz target.
