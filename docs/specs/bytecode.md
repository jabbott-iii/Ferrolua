# Subsystem: `ferrolua-bytecode`

`ferrolua-bytecode` defines the instructions Ferrolua's compiler emits and its VM runs, the
function prototypes that hold them, the binary format of precompiled chunks, and the verifier that
checks a chunk before it runs. The format is Ferrolua's own, and chunks compiled by PUC-Rio's
`luac` don't load (ADR-0010, proposed: Ferrolua's own register bytecode, with every precompiled
chunk verified). It lands in M2.1; the chunk format and the verifier land in M3.12.

## Why a register bytecode of Ferrolua's own

A register machine names its operands by slot in the function's frame, as PUC-Rio Lua's has since
5.0, so `a = b + c` is one instruction instead of the four a stack machine needs. Ferrolua keeps
that design but not PUC-Rio's encoding: its VM needs instructions PUC-Rio lacks, for reference
counting, class fields, strict arity checks, and JIT hints, and PUC-Rio's format changes with
each release. A precompiled chunk can come from anywhere, so the verifier treats it as hostile.

## Instructions

Each instruction is a 32-bit word with an 8-bit opcode. The other 24 bits hold operands in one of
these forms:

| Form | Fields |
| :- | :- |
| `ABC` | three 8-bit operands: usually a destination register and two sources |
| `ABx` | an 8-bit register and a 16-bit unsigned operand, such as a constant index |
| `AsBx` | an 8-bit register and a 16-bit signed operand, such as a jump offset |
| `sJ` | a 24-bit signed jump offset |

An instruction that needs a wider operand is followed by an `EXTRA` word. The instruction set covers
moves and loads, upvalues, globals through `_ENV`, table access with constant-key forms, arithmetic,
bitwise, and comparison with constant operands, membership and identity tests, jumps, calls and tail
calls, returns, varargs, numeric and generic `for`, closures, `<close>` variables, and the
extensions: `ARITY` checks, class construction, and field access by slot. M2.1 records the full
list, with each opcode's operands and effect, in this section.

## Prototypes

```rust
pub struct Prototype {
    pub code: Vec<u32>,
    pub constants: Vec<Constant>,      // nil, booleans, integers, floats, strings
    pub children: Vec<Prototype>,      // nested functions
    pub upvalues: Vec<UpvalueDesc>,    // where each upvalue comes from
    pub params: u8,
    pub optional_params: u8,
    pub is_vararg: bool,
    pub strict_arity: bool,
    pub max_registers: u8,
    pub debug: DebugInfo,              // source name, line of each instruction, local names
    pub hints: Vec<TypeHint>,          // JIT hints from type annotations (ADR-0006)
}
```

## Chunk format

A precompiled chunk starts with the bytes `\x1bFerrolua`, then a format version, then the main
prototype, written depth first. Every integer is little-endian on every platform; every count is
checked against the bytes left before anything is allocated, so a chunk can't ask for more memory
than its own length justifies.

## Verifier

The verifier runs on every chunk `load` reads in binary mode, before any of it runs. It checks:

- every register operand is below the prototype's `max_registers`;
- every constant, upvalue, and child index exists;
- every jump lands on an instruction inside the function, and never in the middle of an
  instruction and its `EXTRA` word;
- every opcode is defined, and an `EXTRA` word follows only instructions that take one;
- the code ends with a return on every path;
- the debug information has one line entry per instruction.

The JIT relies on these facts instead of checking them again (ADR-0013), so a hole in the verifier
is a memory-safety bug.

## Invariants

1. Every prototype the compiler emits passes the verifier.
2. Writing a prototype and reading it back gives an equal prototype.
3. The verifier rejects, with an error and never a panic, every chunk that breaks one of its rules.
4. Reading a chunk never allocates more than a fixed multiple of the chunk's length.

## Tests

- Invariant 1: in debug builds, the compiler's tests run the verifier on every prototype they
  build, and so does the conformance runner.
- Invariant 2: a property test on prototypes compiled from generated programs.
- Invariant 3: hostile-input tests with one test per rule, each breaking exactly that rule, and the
  `chunk` fuzz target, which mutates valid chunks.
- Invariant 4: the `chunk` fuzz target runs under a counting allocator with a limit of 64 times the
  input's length.
