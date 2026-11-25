# Subsystem: `ferrolua-vm`

`ferrolua-vm` runs bytecode. It owns the heap of reference-counted values and the deterministic
loop collector ([`memory.md`](memory.md)), the tables, the interpreter loop, metamethods, error
handling, coroutines, the limits of [`sandbox.md`](sandbox.md), and the hooks the debugger uses.
Where the language keeps Lua 5.5's behavior, it matches PUC-Rio Lua 5.5.1's results and error
messages, and it contains no `unsafe` code (ADR-0011, proposed: the heap is safe Rust with
generational handles). It doesn't depend on the JIT: it defines a trait
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
seeded per state: from the standard library's `RandomState` by default, so a script can't choose
keys that collide, or from `Options::hash_seed` when a host needs runs to repeat exactly
([`ferrolua.md`](ferrolua.md)).

ADR-0032 (proposed: grow strings and lists in place when only one place holds them) would let
`s = s + t` and `s += t` on a local append to `s` in place when its count shows nothing else holds
it and it is longer than 40 bytes, so it isn't interned. The result is the value a copy would give,
the append drops the cached hash, and lists follow the same rule (M4.9).

## Tables

A table has an array part for keys 0 to n − 1 and a hash part for the rest (ADR-0023: a language
of its own, with Python's operators and 0-based indexes). The
hash part uses open addressing with the seeded hash. Float keys with integer values, such as
`2.0`, are stored as integers, and `nil` and NaN keys raise PUC-Rio's errors.

- A table, a function, a thread, or a userdata used as a key is hashed from its handle, its slot
  and generation, mixed with the seed, never from an address. Under a fixed seed, `next` then
  visits every kind of key in the same order in every run (ADR-0020).
- No set of keys makes one table operation run long, even under a seed a script knows (ADR-0020):
  each probe step past the first, in a lookup, an insertion, or `next`, counts as one work unit
  against the instruction limit of [`sandbox.md`](sandbox.md#limits). The owner chose counting
  over a cap on 2026-11-24. A cap would also bound a host that sets no limit, but it needs a
  fallback structure for keys that share a full hash, and more code in every table operation.

- `#t` returns a border: an n where `t[n - 1]` isn't `nil` and `t[n]` is, or 0 when `t[0]` is
  `nil`. When a table has several borders, Ferrolua returns the one PUC-Rio Lua 5.5.1's algorithm
  returns for the same table with every integer key one higher, so `#{1, nil, 3}` gives what
  PUC-Rio gives for its own `{1, nil, 3}`.
- `next` visits the array part in order, then the hash part, and continues correctly while the
  loop assigns `nil` to visited fields, as §6.1 of the manual allows.
- Each table keeps a count of its entries, so a truth test knows in one step whether a table is
  empty (ADR-0025: Python's truth).

## Lists, tuples, and sets

Lists, tuples, and sets are values of their own, with an arena each
([`memory.md`](memory.md#objects-and-arenas)), as
[`language.md`](language.md#lists-tuples-and-sets-adr-0025) defines them (ADR-0025: lists, tuples,
and sets beside tables).

- A list is a growable array of values, `nil` included, with its length; a tuple is a fixed array
  that never changes, with its hash computed on first use. Indexing either checks the index
  against the length, so it never reads past the end.
- A set is a hash set over the same seeded hash as tables, with reference values hashed by handle
  (ADR-0020) and tuples by value, and the same probe-step counting against the instruction limit.
- Each type has one metatable per state, which scripts can read with `getmetatable` but not
  replace.
- Joining, repeating, and comparing collections, and the set operators, count one work unit per
  element they visit or produce, as library calls count their work
  ([`sandbox.md`](sandbox.md#limits)).

## The interpreter

The interpreter is a loop that decodes one instruction and runs it, with a `match` on the opcode.
A call pushes a frame with a base register and a return address; a tail call reuses the frame, so
proper tail calls use no stack. Native functions, the ones written in Rust, are called with a view
of their argument registers and return values the same way.

Operators follow [`language.md`](language.md#meanings): integer operations wrap around on overflow,
`//` and `%` round toward minus infinity, `**` of two integers gives an integer when the exponent
isn't negative, `>>` keeps the sign, division by zero raises an error, and nothing converts between
strings and numbers. A truth test follows [`language.md`](language.md#truth-adr-0025): it checks the
value's type, then for a table its count of entries and its metatable's `__bool`. M2.9 records each
operator's edge cases and error messages here. Number-to-string conversion prints floats with enough
digits to read back the same value, as Lua 5.5 does, using PUC-Rio's exact rules, which M2.9 records
here.

ADR-0031 (proposed: specialize instructions in place) would let the interpreter rewrite an
instruction that keeps seeing the same kinds of operands into a form for them, such as an integer
add, and rewrite it back when they change. Both forms give the same results and count the same
work units, so limits stop a script at the same point, and specialized opcodes never leave the
state: `string.dump` writes the generic ones (M5.10).

## Metamethods

Every event in §2.4 of the manual is supported: arithmetic, bitwise, `__len`, `__eq`, `__lt`,
`__le`, `__index`, `__newindex`, `__call`, `__close`, `__gc`, `__mode`, `__name`, `__tostring`,
`__metatable`, and `__pairs`, apart from `__concat`, which went with `..`. Three are new:
`__contains`, for `in` (ADR-0024: the details Python's operators and 0-based indexes leave open),
and `__bool`, for truth, and `__format`, for an f-string field with a spec (ADR-0026:
the details of f-strings, locals, collections, and truth). A chain of
`__index` or `__newindex` tables stops at PUC-Rio Lua 5.5.1's limit, with its message, which M2.10
records here. A chain of `__call` stops after 15 objects, as §8.1 of the manual says.

## Errors

A Lua error is a value carried in `Err` back through each frame, never a panic. Each frame on the
way closes its `<close>` variables. `pcall` and `xpcall` catch errors, except the limit errors of
[`sandbox.md`](sandbox.md). Runtime messages for the errors Ferrolua shares with Lua 5.5 match
PUC-Rio Lua 5.5.1's, including the variable description the bytecode allows, as in
`attempt to index a nil value (global 'config')` and `attempt to call a nil value (method 'draw')`.
An error with a `nil` value becomes a string, as Lua 5.5 does.

ADR-0029 (proposed: runtime errors with their source line, a caret, and a hint) would keep each
message as it is and add, beside it, the span of the failing expression from the debug
information and, once the error reaches the host, a hint such as "did you mean `print`?". The VM
works out no hint while the script runs, so `pcall` and its cost don't change (M2.20).

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
4. Every runtime error message that Lua 5.5 shares matches what PUC-Rio Lua 5.5.1 gives for the
   conformance script's reference twin.
5. `#t` returns the border PUC-Rio Lua 5.5.1 returns for the table the reference twin builds, whose
   integer keys are each one higher.

## Tests

- Invariants 1, 4, and 5: the conformance scripts, which print error messages caught with `pcall`
  and lengths of tables with holes.
- Invariant 2: the `interpreter` fuzz target runs generated programs under instruction and memory
  limits.
- Invariant 3: `tests/conformance/tail_calls.lua`, and a unit test that measures the frame count.
