# Subsystem: `ferrolua-compiler`

`ferrolua-compiler` turns the compact tree from `ferrolua-syntax` into a `Prototype` in
`ferrolua-bytecode`'s format. It resolves every name as a local, an upvalue, or a global under Lua
5.5's `global` rules; applies strict mode where it is on (ADR-0008, proposed: `--!strict` or a host
option); compiles self-recursion without an upvalue, so it makes no loop for the collector to find
(ADR-0018: reference counting plus a deterministic loop collector); folds constants; and allocates
registers. It reports compile errors in PUC-Rio's wording. It lands in M2.5–M2.7; compound
assignment, classes, and strict mode land in M4.

## Why a separate compiler pass

PUC-Rio Lua parses and emits code in one pass, with no tree. Ferrolua compiles from a tree because
the parser is shared with the tools (ADR-0009), and because some decisions need the whole
function first: whether a local function is ever reassigned, which locals a closure captures, and
which constants are never written. The cost is the compact tree's memory during `load`, which
the parser's benchmarks measure.

## Name resolution

Names resolve by the rules of [`language.md`](language.md#scoping-adr-0025) (ADR-0025: locals by
default, for the whole function). Before it emits a function's code, the compiler walks the
function's body once to collect the names it assigns and its `global` and `nonlocal` declarations,
so it knows each function-wide local before its first use. Each name then resolves, from the
innermost scope out, to:

- a block-scoped local or a function-wide local of the current function, which lives in a register;
- a local of an enclosing function, which becomes an upvalue;
- a global, which compiles to an access to the `_ENV` upvalue, as §2.2 of the manual describes.

A function-wide local starts out unassigned. Where the compiler can't prove that every path to a
read assigns the local first, it emits a check that raises `local 'x' read before assignment`; a
local that every path assigns first gets no check. A `global` or `nonlocal` declaration after the
function's first use of its name is a compile error, and so is a `nonlocal` that names no enclosing
function's variable. In strict mode, the chunk starts with a declaration of the names the host
lists, and reading any other undeclared name is a compile error
([`language.md`](language.md#undeclared-variable-errors)). Assigning to a `<const>` variable or a
`for` control variable is a compile error, as is assigning to a `global <const>`.

## Self-recursion

For each `local function f`, and each function-wide local whose one assignment is a function, such
as `function f() ... end` at a chunk's top level (ADR-0026), the compiler checks every
assignment to `f` in its scope. If the
definition is the only one, every use of `f` inside its own body compiles to `SELF`, which loads
the running closure, and `f` gets no upvalue. Closures nested inside the body that use `f` capture
it as an ordinary upvalue of the outer closure. [`memory.md`](memory.md#recursion-without-a-loop)
explains why this is sound.

## Registers and expressions

Registers are allocated in a stack discipline: locals take the lowest free registers in order, and
temporaries sit above them and are released at the end of each expression. A function may use
at most 255 registers; more raises `function or expression needs too many registers`, as in
PUC-Rio. Constant folding evaluates arithmetic and bitwise operations on numeric constants at
compile time, and `+` on string constants, except where the operation would raise an error at run
time: division by zero, a negative shift count, a float in a bitwise operation, or a string with a
number ([`language.md`](language.md#meanings)). `<const>` locals with constant values are folded
too.

A chained comparison such as `a < b < c` evaluates each middle operand once into a register and
jumps past the rest of the chain at the first comparison that is false. A condition, in an `if`, a
loop, `and`, `or`, `not`, or a conditional expression, compiles to a test of truth as
[`language.md`](language.md#truth-adr-0025) defines it. An f-string compiles each field's expression
in order, then one instruction that formats every field and joins the parts, so a long f-string
makes one string instead of one per part. A list or tuple literal compiles to one instruction that
builds the collection from its registers, as a table constructor does. A conditional expression
`x if c else y` compiles to a test of `c` and two branches that each write the same register.

## Statements

Each statement in §3.3 of the manual compiles as PUC-Rio's does, including `goto` and labels with
Lua 5.5's rules on jumping into a local's scope, generic `for` with its closing value, and `<close>`
variables, which run `__close` on every exit from their block. The numeric `for`, with integer and
float loops, stops before its end ([`language.md`](language.md#indexes-and-ranges-adr-0023)).
Compound assignment evaluates the target's parts once
([`language.md`](language.md#compound-assignment-adr-0023)).

## Type hints

Where an annotation gives a local or parameter a primitive type, such as `integer` or
`{number}`, the compiler records a hint in the prototype, so the JIT can guard on that type first.
Hints never change what the bytecode does.

## Invariants

1. Every prototype it emits passes the bytecode verifier.
2. Compiling the same source twice gives the same bytes.
3. A self-recursive local function that isn't reassigned has no upvalue for its own name.
4. Folding a constant gives the value the VM would compute, or isn't done.
5. Every compile error in code Ferrolua shares with Lua 5.5, Ferrolua reports with PUC-Rio
   Lua 5.5.1's message and line.

## Tests

- Invariants 1 and 2: the conformance runner verifies and compiles each script twice.
- Invariant 3: unit tests inspect the prototypes of recursive, reassigned, and mutually recursive
  functions.
- Invariant 4: a property test evaluates generated constant expressions both folded and at run
  time, including `math.mininteger // -1`, `1 // 0`, `2 ** 63`, `-1 >> 1`, and `1 << -1`.
- Invariant 5: `tests/conformance/compile_errors.lua` loads each error case with `load` and prints
  the message.
