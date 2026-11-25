# Subsystem: `ferrolua-jit`

`ferrolua-jit` compiles hot Lua functions to x86-64 machine code while they run. It has an encoder
for x86-64 instructions, executable memory that is never writable and executable at once, a
baseline tier that translates bytecode instruction by instruction, and an optimizing tier that
works on SSA form (ADR-0012, proposed: a method JIT with tiers). It is the only crate that may use
`unsafe`, and only in `jit::memory` and `jit::entry`, under the six invariants of ADR-0013
(proposed). A host links it only by turning on the `ferrolua` crate's `jit` feature, which is off
by default (ADR-0022: the JIT is an optional feature of `ferrolua`, off by default). Generated code
always produces what the interpreter produces, and exits to the interpreter whenever a guard fails.
The baseline tier lands in M5, and the optimizing tier in M11.

## Why a method JIT with exits to the interpreter

A method JIT compiles a whole function at a time. It handles code with no loops, such as event
callbacks, as well as code with loops, and its output is straightforward to compare with the
interpreter's. A tracing JIT, like LuaJIT's, reaches higher speeds on loops, but recording and
stitching traces is harder to make correct. Ferrolua chose correctness first. Every compiled
function can stop at any guard and continue in the interpreter, so the JIT only compiles the cases
it has seen and leaves the rest to the interpreter. The cost is peak loop speed.

## The encoder

`jit::asm` writes the subset of x86-64 that the tiers use: moves, loads and stores with base, index,
and displacement, integer and scalar double arithmetic, comparisons, conditional and unconditional
jumps with labels that are fixed up once the target is known, calls, and returns. It knows nothing
about Lua. Each instruction form has a test with its bytes taken from the Intel 64 and IA-32
Architectures Software Developer's Manual.

## Executable memory

`jit::memory` maps memory read-write with `mmap`, lets the encoder write into it, then calls
`mprotect` to make it read-execute before any code runs. Code is never changed in place: patching
compiled code means writing a new buffer. A buffer is freed only when no prototype and no frame
refers to it. This module and `jit::entry` hold every `unsafe` block in Ferrolua, each with a
`// SAFETY:` comment that names the ADR-0013 invariant it relies on.

## Baseline tier

The baseline tier walks a prototype's bytecode once and emits a template of machine code for each
instruction:

- Values stay in their registers in the VM's frame, so an exit needs no conversion.
- Each operation first checks the types the interpreter has seen there, such as two integers for
  an `ADD`, and the hints from type annotations. When a guard fails, the code exits to the
  interpreter at that instruction.
- Reference counts are updated exactly as the interpreter updates them, including recording
  candidates for the loop collector, and the collector runs only at safe points, where every
  count is exact.
- Instruction counting for the instruction limit and debug hooks are kept by decrementing a
  counter at each loop's back edge and at each function's entry.
- A loop that gets hot while the interpreter runs it enters compiled code at the loop's head,
  through on-stack replacement.

## Optimizing tier

The optimizing tier builds a static single assignment (SSA) form of a hot function, in which each
value is assigned once. It then optimizes it:

- it propagates constants and types, and removes guards those facts prove;
- it caches table field and class field lookups by shape;
- it inlines small functions;
- it removes pairs of count updates that cancel out;
- it allocates registers by linear scan.

Its exits rebuild the interpreter's frame exactly, counts included, and so does every safe point
where the loop collector can run (ADR-0013, invariant 6).

## Modes

With the `ferrolua` crate's `jit` feature on, a host chooses `JitMode::Off`, `JitMode::On` (the
default on x86-64 Linux), or `JitMode::Forced`, which compiles every function on its first call.
Forced mode is for tests: the conformance runner and the differential fuzz target use it. Without
the feature, `JitMode` still exists, its default is `Off`, and `Lua::new` refuses the other two
with an error that names the feature ([`ferrolua.md`](ferrolua.md)). The `ferrolua` command turns
the feature on.

## Invariants

1. Every script prints the same output and raises the same errors interpreted and in each JIT mode.
2. No mapping is ever writable and executable at once.
3. Every `#[repr(C)]` layout the generated code reads has the size and field offsets the JIT
   expects.
4. At every exit and every safe point, the interpreter's frame and every reference count equal
   what the interpreter alone would have produced.
5. Compiled code collects after the same instructions as the interpreter, and frees the same
   values in the same order ([`memory.md`](memory.md#determinism)).
6. Instruction and memory limits stop compiled code at the same point they stop the interpreter,
   within one loop iteration.

## Tests

- Invariant 1: the conformance and extension scripts in `JitMode::Forced`, and the `differential`
  fuzz target, which runs each generated program in both modes and compares the output.
- Invariant 2: a test compiles functions and reads `/proc/self/maps` while the code exists,
  expecting no mapping with both `w` and `x`.
- Invariant 3: `const` assertions on `size_of` and `offset_of!` for every layout, in `ferrolua-vm`
  and `ferrolua-jit` alike.
- Invariant 4: a debug-build check after each exit compares the frame and counts with a shadow run
  in the interpreter.
- Invariant 5: the conformance runner compares each script's collection log interpreted and in
  `JitMode::Forced`, and the `differential` fuzz target does the same.
- Invariant 6: the limit tests of [`sandbox.md`](sandbox.md), run in `JitMode::Forced`.
