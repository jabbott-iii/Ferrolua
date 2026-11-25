# Subsystem: `ferrolua-vm`

`ferrolua-vm` runs bytecode. It owns the heap of reference-counted values and the deterministic
loop collector ([`memory.md`](memory.md)), the tables, the interpreter loop, metamethods, error handling,
coroutines, the limits of [`sandbox.md`](sandbox.md), and the hooks the debugger uses. It matches
PUC-Rio Lua 5.5.1's results and error messages, and contains no `unsafe` code (ADR-0011, proposed:
the heap is safe Rust with generational handles). It doesn't depend on the JIT: it defines a trait
that a compiled tier implements, and the `ferrolua` crate plugs the JIT in. It lands in M2.2–M2.12.

## Why an interpreter in safe Rust

The interpreter is the reference for every JIT tier, and the tier that runs everything the JIT
doesn't compile. Writing it in safe Rust means a bug in it can produce a wrong result or an
internal error, but can't corrupt memory. The cost is bounds and generation checks that a C
interpreter wouldn't make. The interpreter keeps them cheap by checking a frame's register range
once on entry.

## Strings

Strings are immutable byte strings. Strings of 40 bytes or fewer are interned: the state keeps one
copy of each, in a table keyed by content, so comparing two short strings compares their handles.
Longer strings are compared by content, and their hash is computed on first use. The hash is
seeded per state from the standard library's `RandomState`, so a script can't choose keys that
collide.

## Tables

A table has an array part for keys 1 to n and a hash part for the rest, as in PUC-Rio Lua. The
hash part uses open addressing with the seeded hash. Float keys with integer values, such as
`2.0`, are stored as integers, and `nil` and NaN keys raise PUC-Rio's errors.

- `#t` returns a border: an n where `t[n]` isn't `nil` and `t[n + 1]` is, or 0 when `t[1]` is
  `nil`. When a table has several borders, Ferrolua returns the one PUC-Rio Lua 5.5.1's algorithm
  returns, so `#{1, nil, 3}` gives what PUC-Rio gives.
- `next` visits the array part in order, then the hash part, and continues correctly while the
  loop assigns `nil` to visited fields, as §6.1 of the manual allows.

## The interpreter

The interpreter is a loop that decodes one instruction and runs it, with a `match` on the opcode.
A call pushes a frame with a base register and a return address; a tail call reuses the frame, so
proper tail calls use no stack. Native functions, the ones written in Rust, are called with a view
of their argument registers and return values the same way.

Integer and float arithmetic follows §3.4.1 of the manual: integer operations wrap around on
overflow, `//` and `%` round toward minus infinity, and strings convert to numbers in arithmetic.
Number-to-string conversion prints floats with enough digits to read back the same value, as
Lua 5.5 does, using PUC-Rio's exact rules, which M2.9 records here.

## Metamethods

Every event in §2.4 of the manual is supported: arithmetic, bitwise, `__concat`, `__len`, `__eq`,
`__lt`, `__le`, `__index`, `__newindex`, `__call`, `__close`, `__gc`, `__mode`, `__name`,
`__tostring`, `__metatable`, and `__pairs`. A chain of `__index` or `__newindex` tables stops at
PUC-Rio Lua 5.5.1's limit, with its message, which M2.10 records here. A chain of `__call` stops
after 15 objects, as §8.1 of the manual says.

## Errors

A Lua error is a value carried in `Err` back through each frame, never a panic. Each frame on the
way closes its `<close>` variables. `pcall` and `xpcall` catch errors, except the limit errors of
[`sandbox.md`](sandbox.md). Runtime messages match PUC-Rio Lua 5.5.1's, including the variable
description the bytecode allows, as in `attempt to index a nil value (global 'config')` and
`attempt to call a nil value (method 'draw')`. An error with a `nil` value becomes a string, as
Lua 5.5 does.

## Coroutines

Each coroutine has its own register stack and frame list in the threads arena. `resume` switches
the interpreter to the coroutine's frames, and `yield` switches back, so a yield needs no native
stack of its own. A native function that calls back into Lua, such as `pcall`, `table.sort`'s
comparator, or a `string.gsub` callback, may let the callback yield only where PUC-Rio allows it.
`pcall` and `xpcall` allow it; the others raise `attempt to yield across a C-call boundary`.

## Debug hooks

The VM can call a hook on each new line, each call, each return, and every n instructions, as
`debug.sethook` describes. The debugger ([`dap.md`](dap.md)) uses the same hooks, plus line
breakpoints, which the interpreter checks only in functions that have one. A function with a
breakpoint always runs interpreted.

## The compiled tier

```rust
pub trait Tier {
    fn on_hot(&mut self, vm: &mut Vm, proto: ProtoHandle);
    fn run(&mut self, vm: &mut Vm, frame: FrameId) -> Result<Exit, LuaError>;
}
```

The interpreter calls `on_hot` when a function's call or loop counter passes its threshold, and
calls `run` when a function has compiled code. `Exit` says where the interpreter continues.
[`jit.md`](jit.md) implements it.

## Invariants

1. Every conformance script prints its recorded output.
2. No input, script, or bytecode panics the VM.
3. A proper tail call in a loop runs in constant stack: 10,000,000 tail calls use the stack of one.
4. Every runtime error message matches PUC-Rio Lua 5.5.1's for the same script.
5. `#t` returns the border PUC-Rio Lua 5.5.1 returns, for every table built by the same script.

## Tests

- Invariants 1, 4, and 5: the conformance scripts, which print error messages caught with `pcall`
  and lengths of tables with holes.
- Invariant 2: the `interpreter` fuzz target runs generated programs under instruction and memory
  limits.
- Invariant 3: `tests/conformance/tail_calls.lua`, and a unit test that measures the frame count.
