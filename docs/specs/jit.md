# Subsystem: `ferrolua-jit`

`ferrolua-jit` compiles hot Lua functions to x86-64 or AArch64 machine code while they run, on
Linux, Windows, macOS, and Android (ADR-0027: a JIT wherever the platform allows one; iOS and the
web run interpreted). It has an encoder for each processor, executable memory that is never
writable and executable at once, a
baseline tier that translates bytecode instruction by instruction, and an optimizing tier that
works on SSA form (ADR-0012, proposed: a method JIT with tiers). It is the only crate that may use
`unsafe`, and only in `jit::memory` and `jit::entry`, under the seven invariants of ADR-0013
(proposed). A host links it only by turning on the `ferrolua` crate's `jit` feature, which is off
by default (ADR-0022: the JIT is an optional feature of `ferrolua`, off by default). Generated code
always produces what the interpreter produces, and exits to the interpreter whenever a guard fails.
The baseline tier lands in M5 for x86-64 on Linux and in M13 for every other target, and the
optimizing tier in M11.

## Why a method JIT with exits to the interpreter

A method JIT compiles a whole function at a time. It handles code with no loops, such as event
callbacks, as well as code with loops, and its output is straightforward to compare with the
interpreter's. A tracing JIT, like LuaJIT's, reaches higher speeds on loops, but recording and
stitching traces is harder to make correct. Ferrolua chose correctness first. Every compiled
function can stop at any guard and continue in the interpreter, so the JIT only compiles the cases
it has seen and leaves the rest to the interpreter. The cost is peak loop speed.

## The encoders

`jit::asm` writes the subset of x86-64 (M5.2) and of AArch64 (M13.1) that the tiers use: moves,
loads and stores with base, index, and displacement, integer and scalar double arithmetic,
comparisons, conditional and unconditional jumps with labels that are fixed up once the target is
known, calls, and returns. It knows nothing about Lua, and it is ordinary Rust, so its tests run on
every host. Each instruction form has a test with its bytes taken from the Intel 64 and IA-32
Architectures Software Developer's Manual, or from Arm's Architecture Reference Manual.

Calls follow each target's convention: System V on x86-64 Linux, macOS, and Android; Windows x64
on x86-64 Windows; and the AArch64 procedure call standard, with Apple's and Microsoft's variants,
on AArch64. Generated code never calls a platform's math library: `**` and the `math` functions
call `ferrolua-math`, so compiled code gives the interpreter's bits (ADR-0028).

## Executable memory

`jit::memory` maps memory read-write, lets the encoder write into it, then makes it read-execute
before any code runs, with each operating system's calls, which ADR-0013 lists: `mmap` and
`mprotect` on Linux and Android, `VirtualAlloc` and `VirtualProtect` on Windows, and `mmap` with
`MAP_JIT` on macOS, where `pthread_jit_write_protect_np` switches a thread between writing and
running on AArch64. On AArch64 it flushes the instruction cache before a buffer first runs, and on
Windows it registers unwind tables for each buffer. Code is never changed in place: patching
compiled code means writing a new buffer. A buffer is freed only when no prototype and no frame
refers to it. This module and `jit::entry` hold every `unsafe` block in Ferrolua, each with a
`// SAFETY:` comment that names the ADR-0013 invariant it relies on.

## Baseline tier

The baseline tier walks a prototype's bytecode once and emits a template of machine code for each
instruction:

- Values stay in their registers in the VM's frame, so an exit needs no conversion.
- Each operation first checks the types the interpreter has seen there, such as two integers for
  an `ADD`, and the hints from type annotations. When a guard fails, the code exits to the
  interpreter at that instruction. The interpreter's specialized opcodes are that record of types
  (ADR-0031: instructions specialized in place).
- Where the interpreter appends to a string or list in place because only one place holds it,
  compiled code does the same (ADR-0032), so both collect at the same points.
- Reference counts are updated exactly as the interpreter updates them, including recording
  candidates for the loop collector, and the collector runs only at safe points, where every
  count is exact.
- Instruction counting for the instruction limit and debug hooks are kept by decrementing a
  counter at each loop's back edge and at each function's entry. Under ADR-0037 (proposed), code
  that can't finish its next stretch within the budget would hand it to the interpreter, so a
  task pauses at the same instruction compiled or not.
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
default on every target with a backend), or `JitMode::Forced`, which compiles every function on its
first call.
Forced mode is for tests: the conformance runner and the differential fuzz target use it. Without
the feature, `JitMode` still exists, its default is `Off`, and `Lua::new` refuses the other two
with an error that names the feature ([`ferrolua.md`](ferrolua.md)). The `ferrolua` command turns
the feature on.

On iOS and the web, `ferrolua-jit` compiles to nothing, so the feature changes nothing there: the
default is `Off`, and `Lua::new` refuses the other modes with an error that names the target. Where
the operating system refuses executable memory at run time, as macOS does for a hardened host
without the `com.apple.security.cs.allow-jit` entitlement, a state in `JitMode::On` runs
interpreted and tells the host that its JIT isn't running, and `JitMode::Forced` returns the error
(ADR-0028). M13.5 builds both.

## Invariants

1. Every script prints the same output and raises the same errors interpreted and in each JIT mode,
   on every target with a backend.
2. No mapping is ever writable and executable at once for any thread.
3. Every `#[repr(C)]` layout the generated code reads has the size and field offsets the JIT
   expects.
4. At every exit and every safe point, the interpreter's frame and every reference count equal
   what the interpreter alone would have produced.
5. Compiled code collects after the same instructions as the interpreter, and frees the same
   values in the same order ([`memory.md`](memory.md#determinism)).
6. Instruction and memory limits stop compiled code at the same point they stop the interpreter,
   within one loop iteration.
7. On AArch64, a buffer that is rewritten after it ran runs the new code, never the old.

## Tests

- Invariant 1: the conformance and extension scripts in `JitMode::Forced`, and the `differential`
  fuzz target, which runs each generated program in both modes and compares the output.
- Invariant 1: also every target's CI job, which runs the forced pass (M13.6).
- Invariant 2: on Linux and Android, a test compiles functions and reads `/proc/self/maps` while the
  code exists, expecting no mapping with both `w` and `x`; on Windows, a test queries each buffer
  with `VirtualQuery`; on macOS for AArch64, a test checks that a thread can't write a buffer while
  it runs one.
- Invariant 3: `const` assertions on `size_of` and `offset_of!` for every layout, in `ferrolua-vm`
  and `ferrolua-jit` alike.
- Invariant 4: a debug-build check after each exit compares the frame and counts with a shadow run
  in the interpreter.
- Invariant 5: the conformance runner compares each script's collection log interpreted and in
  `JitMode::Forced`, and the `differential` fuzz target does the same.
- Invariant 6: the limit tests of [`sandbox.md`](sandbox.md), run in `JitMode::Forced`.
- Invariant 7: a test on each AArch64 target writes, runs, rewrites, and runs a buffer again, and
  checks the second result (M13.2).
