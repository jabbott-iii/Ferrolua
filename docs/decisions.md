# Decision Log

This log records each significant or hard-to-reverse decision about Ferrolua as an architecture
decision record (ADR): what was decided, why, and what would change it. Records are numbered in the
order they were made, and an accepted record's decision is never edited: a new record supersedes
it. A record is `Accepted` only when the project's owner made or approved the decision; until then
it is `Proposed`, and no code may depend on it.

## ADR-0001 — Branch from Lua 5.5

- **Status:** Accepted; ADR-0023 makes Ferrolua a language of its own, so PUC-Rio Lua 5.5.1 is the
  reference only for what Ferrolua keeps from Lua 5.5, through paired scripts
- **Date:** 2026-11-24
- **Context:** Ferrolua implements an existing Lua version, so scripts written for it run on
  Ferrolua and its behavior can be checked against a reference. Lua 5.5 was released on
  December 22, 2025, and its current release is 5.5.1, from August 3, 2026, as
  [lua.org's version history](https://www.lua.org/versions.html) lists. The same page says Lua 5.4
  ends at 5.4.9: "There will be no further releases of Lua 5.4." Lua 5.5 adds `global`
  declarations, which make an undeclared name an error inside their scope. That gives Ferrolua's
  undeclared-variable errors a standard base to build on (ADR-0004).
- **Decision** (the owner chose Lua 5.5 on 2026-11-24): Ferrolua branches from Lua 5.5. The
  reference is PUC-Rio Lua 5.5.1, the interpreter that Lua's authors at the Pontifical Catholic
  University of Rio de Janeiro (PUC-Rio) publish, and the
  [Lua 5.5 Reference Manual](https://www.lua.org/manual/5.5/). A valid Lua 5.5 script must print
  on Ferrolua what it prints on PUC-Rio Lua 5.5.1, apart from the differences other records name.
- **Consequences:**
  - Good: the baseline is the current Lua release, which still gets fixes, and its `global`
    declarations match the strict mode Ferrolua wants.
  - Good: conformance has one reference interpreter to record expected output from.
  - Bad: Lua 5.1 and LuaJIT code that relies on `setfenv`, `module`, or `unpack` as a global doesn't
    run unchanged, and much of the Lua ecosystem targets 5.1.
  - Bad: `global` is a reserved word in 5.5, so a 5.4 script that names a variable `global` doesn't
    parse.
  - Follow-up: the rules in `.claude/rules/`, `CONTRIBUTING.md`, and the issue forms name Lua 5.5
    as the baseline.
- **Alternatives considered:**
  - Lua 5.4: it gets no further releases, and it lacks `global` declarations.
  - Lua 5.1 with LuaJIT's extensions: it is the version most existing Lua code targets, but it
    lacks integers, `goto`, `<close>`, and `global`, and its reference implementation no longer
    changes.
- **Revisit when:** lua.org releases Lua 5.6, or a Lua 5.5 bug-fix release changes the output of
  a conformance script.

## ADR-0002 — Use no third-party code

- **Status:** Accepted
- **Date:** 2026-11-24
- **Context:** Ferrolua runs scripts its hosts didn't write, so every line of code in it is part
  of its attack surface. A dependency brings in code that its owners can change in any release,
  and a dependency tree grows with each update. The owner set the rule in
  `.claude/rules/dependencies.md`, and [`02-dependency-policy.md`](02-dependency-policy.md) explains
  it for contributors.
- **Decision** (the owner chose no third-party crates at all, dev-dependencies included, on
  2026-11-24): Ferrolua uses only the Rust toolchain, its standard library, PUC-Rio Lua 5.5 from
  lua.org as the reference for tests, Linux, Git, and GitHub. It uses no third-party crate, even for
  tests, linting, formatting, benchmarks, coverage, or fuzzing. An exception needs its own accepted
  record first.
- **Consequences:**
  - Good: `cargo tree` lists only Ferrolua's own crates, so a host that embeds Ferrolua adds no
    one else's code.
  - Good: no update can bring in a package the project didn't review.
  - Bad: Ferrolua writes its own JSON parser, x86-64 assembler, float formatting, argument parser,
    property-test helper, and fuzzer. [`00-overview.md`](00-overview.md) estimates their size.
  - Bad: tools many Rust projects use, such as cargo-llvm-cov, cargo-mutants, cargo-semver-checks,
    and libFuzzer, are out. The toolchain's `llvm-tools` component stands in for coverage.
- **Alternatives considered:**
  - Allow dev-dependencies only: test tools would still run in CI with access to the source and
    its secrets.
  - Allow a short list of widely used crates: popularity isn't evidence that a crate is safe, and
    the list would grow.
- **Revisit when:** a law, a contract, or a platform requires a specific library, or Ferrolua needs
  cryptography that the standard library doesn't provide.

## ADR-0003 — Free memory by reference counting, with no garbage collector

- **Status:** Superseded by ADR-0018
- **Date:** 2026-11-24
- **Context:** PUC-Rio Lua frees memory with a tracing garbage collector: it pauses now and then to
  find every value still reachable from the program, and frees the rest. Ferrolua aims for the
  memory model Rust programs use instead: a value is freed at a known point, and freeing never
  waits for a collector to run. Reference counting does this. Each value counts the references to
  it and is freed when the count reaches zero, the model behind Rust's `Rc`. Reference counting
  can't free a loop of values that refer to each other. Lua code makes such loops often: a recursive
  local function refers to itself, the usual class pattern `Class.__index = Class` points a table at
  itself, and a tree with parent pointers links each child back to its parent.
- **Decision** (the owner chose the first of three options on 2026-11-24):
  - Every value is reference counted, and is freed as soon as its count reaches zero.
  - Every value belongs to one Lua state, so dropping the state frees every value left, loops
    included.
  - A local function's references to itself compile to "the running function" and hold no count,
    so recursion makes no loop.
  - Classes built with Ferrolua's class syntax (ADR-0007) need no self-reference.
  - The linter reports the loop patterns it can find.
  - Ferrolua has no collector of any kind, including one that runs only on request.
  - [`specs/memory.md`](specs/memory.md) gives the details.
- **Consequences:**
  - Good: memory is freed at a predictable point, with no collection pauses, and `__gc` finalizers
    run at a predictable point too.
  - Good: the heap needs no `unsafe` code (ADR-0011).
  - Bad: a loop the script builds stays in memory until the state is dropped. A long-running host,
    such as a game that keeps one state for hours, grows if its scripts keep building loops, until
    the state's memory limit stops them.
  - Bad: every copy of a reference updates a count, which costs time on each move between
    registers. The optimizing JIT (M11) removes counts it can prove redundant.
  - Bad: this is an intended difference from PUC-Rio Lua 5.5.1. `collectgarbage("collect")` frees no
    loops, and a `__gc` finalizer may run earlier than in PUC-Rio, or, for a value in a loop, only
    when the state is dropped. Weak tables whose values refer back to their own keys keep both
    alive. Conformance scripts that depend on collection timing go in `tests/extensions/`, with
    output written from the spec.
- **Alternatives considered:**
  - Reference counting plus a loop sweep that runs only when the host or the script asks: it
    frees loops in long-running hosts, but the sweep is a collector, which the owner ruled out.
  - One arena per state, freed only when the state is dropped: allocation is fastest, but memory
    grows with every allocation, so only short scripts could use it.
  - A tracing garbage collector, as PUC-Rio Lua has: it frees loops, but it pauses at times the
    host doesn't choose, and its heap needs `unsafe` code.
- **Revisit when:** a host reports that loops its scripts can't avoid exhaust a state's memory
  limit, or benchmarks in M5.1 show that updating counts costs more than a third of the
  interpreter's run time.

## ADR-0004 — Extend Lua 5.5 as a superset, with strict checks turned on per chunk

- **Status:** Superseded by ADR-0023, which keeps strict mode turned on per chunk
- **Date:** 2026-11-24
- **Context:** Ferrolua adds optional types, classes, compound assignment and `++`, arity
  enforcement, and undeclared-variable errors to Lua 5.5. The new syntax is a syntax error in
  Lua 5.5 today, so accepting it changes no valid script. The two checks are different: Lua 5.5
  lets a caller pass any number of arguments and lets code read any global, so turning the checks
  on changes how valid scripts run.
- **Decision** (the owner chose the first of three options on 2026-11-24): Ferrolua's language is
  a superset of Lua 5.5. Every valid Lua 5.5 script runs unchanged, and the new syntax works in
  every chunk. Arity enforcement and undeclared-variable errors, together called strict mode, are
  off unless the host turns them on for a chunk or the chunk turns them on with a directive
  (ADR-0008).
- **Consequences:**
  - Good: conformance with PUC-Rio Lua 5.5.1 stays a hard promise, and existing Lua code runs
    without changes.
  - Good: a project can move to strict mode one file at a time.
  - Bad: a file that uses Ferrolua's syntax doesn't run on PUC-Rio Lua, so the repository's
    conformance scripts can't use it.
  - Bad: every new token and keyword must be checked against valid Lua 5.5, which a property
    test in M4.1 does.
- **Alternatives considered:**
  - Strict by default, with a Lua 5.5 mode to opt into: existing code would fail until it opted
    out.
  - A separate dialect with no promise to run Lua 5.5 scripts: it would lose the reference
    interpreter that conformance tests compare against.
- **Revisit when:** a future Lua release adds syntax that conflicts with one of Ferrolua's
  extensions.

## ADR-0005 — Add `++` and compound assignment, but no `--` or XOR-assign

- **Status:** Superseded by ADR-0023
- **Date:** 2026-11-24
- **Context:** C, Java, and JavaScript programmers expect `x += 1` and `x++`. In Lua, `--` starts a
  comment, so `x--` can't mean "decrement" without breaking every comment written after code on a
  line. Lua also uses `~=` for "not equal", so it can't mean XOR-assign.
- **Decision** (the owner chose the first of three options on 2026-11-24): Ferrolua adds `x++` and
  the compound assignments `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `^=`, `..=`, `&=`, `|=`, `<<=`, and
  `>>=`. Decrement is written `x -= 1`, and there is no XOR-assign. Each is a statement, like Lua's
  own assignment, and none is an expression.
- **Consequences:**
  - Good: comments and `~=` keep their Lua meaning, so the superset promise (ADR-0004) holds.
  - Good: because they are statements, `a[i++] = b` can't be written, which removes the
    order-of-evaluation puzzles C has.
  - Bad: `++` has no matching decrement, which surprises C programmers.
- **Alternatives considered:**
  - Compound assignment only: symmetric, but drops the increment that loops use most.
  - `++` and `--`, with a new comment marker in Ferrolua files: it breaks every existing Lua file.
- **Revisit when:** a future Lua release gives `++` or a compound operator a meaning of its own.

## ADR-0006 — Write optional types inline, check them before run, and use them only as guarded hints

- **Status:** Accepted; under ADR-0023, plain Lua 5.5 files no longer parse as Ferrolua, so the
  LuaCATS annotations it reads serve code ported from Lua 5.5
- **Date:** 2026-11-24
- **Context:** Optional types let a program declare what kind of value a variable holds, so a tool
  can report mistakes before the program runs. Untyped code keeps working, which is the "optional"
  part: this is gradual typing. Types can be written in new syntax or in comments, and a runtime
  can check them on every call or not at all.
- **Decision** (the owner chose the first of three options on 2026-11-24):
  - Types are written inline, as in `local x: number` and `function f(a: number): string`, the form
    that Luau and TypeScript use.
  - The type checker reports type errors before a script runs, and it also reads LuaCATS
    annotations in comments, such as `---@param`, so plain Lua 5.5 files get checked too.
  - The compiler drops the annotations, so they don't change what a script does.
  - The JIT may use them as hints, but checks each one with a guard before relying on it, and never
    trusts a type for memory safety.
  - [`specs/language.md`](specs/language.md) gives the syntax, and [`specs/types.md`](specs/types.md)
    the type system.
- **Consequences:**
  - Good: types cost nothing at run time, and a wrong annotation can't crash the VM.
  - Good: the language server (M9) uses the types for completion.
  - Bad: a value from untyped code can still break a typed function at run time, because nothing
    checks types when a function is called.
- **Alternatives considered:**
  - LuaCATS comments only: every file stays valid Lua 5.5, but types are wordier to write.
  - Inline types also checked on every call: it catches untyped callers, at a cost on every typed
    call.
- **Revisit when:** users report bugs that a runtime check on typed function boundaries would have
  caught, often enough to be worth that check's measured cost.

## ADR-0007 — Add classes as a declaration with a fixed field layout

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** Lua has no classes. Programs build them from tables and metatables, usually with
  `Class.__index = Class`, a table that refers to itself, which reference counting alone can't free,
  so only the loop collector (ADR-0018) frees it. Every library builds classes a little differently,
  so tools can't tell what fields an object has. A class declaration in the language fixes both
  problems.
- **Decision:** Ferrolua adds a `class` declaration with fields, methods, single inheritance with
  `extends`, a constructor named `new`, and `super` calls, as
  [`specs/language.md`](specs/language.md) describes. `class` is a keyword only at the start of a
  statement and followed by a name, so a function named `class` keeps working.
  - Each class is a runtime object, and calling it creates an instance.
  - Each instance refers to its class with a counted reference, and nothing refers back, so no loop
    forms.
  - Each class declares its fields, so instances have a fixed layout. The JIT can then read a field
    at a known offset after one check of the instance's class, and the type checker knows every
    field.
- **Consequences:**
  - Good: classes make no reference loops, so an instance is freed as soon as it's dropped, without
    waiting for the loop collector, and tools see the full shape of every class.
  - Good: field access on an instance can be faster than a table lookup.
  - Bad: instances aren't plain tables, so code that calls `pairs` or `rawget` on an instance needs
    the rules in the spec.
  - Bad: the class syntax is the largest extension to the grammar.
- **Alternatives considered:**
  - A standard library function such as `class("Name")`: no new syntax, but tools can't see the
    shape, and the result is still a table that refers to itself.
  - Classes as plain tables with generated metatables: compatible with `pairs`, but every instance
    lookup stays a hash lookup, and the self-reference stays.
- **Revisit when:** the owner accepts or changes this record, which M4.4 needs first.

## ADR-0008 — Turn strict mode on with `--!strict` or a host option

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** ADR-0004, and ADR-0023 after it, make arity enforcement and undeclared-variable
  errors opt-in per chunk, by the host or by a directive in the file. Lua 5.5's `global`
  declarations already make an undeclared name an error inside their scope: a chunk that starts
  with `global none` may use only the names it declares.
- **Decision:**
  - A chunk turns strict mode on with a first line of `--!strict`. It is a comment, so a tool that
    doesn't know the directive reads an ordinary comment.
  - A host turns it on with an option when it loads a chunk, and can turn on either check alone.
  - Undeclared-variable errors work as if the chunk started with a `global` declaration that lists
    only the names the host registered. Using any other free name without declaring it is a
    compile-time error.
  - Arity enforcement makes a function compiled in strict mode raise an error when it's called with
    more or fewer arguments than its parameters. It counts `self` for methods, and doesn't count
    parameters marked optional with `?` or a vararg `...`. The check runs in the called function,
    so it holds whoever calls it.
  - [`specs/language.md`](specs/language.md) gives the details.
- **Consequences:**
  - Good: turning strict mode on or off changes one line and no code.
  - Good: undeclared-variable errors reuse Lua 5.5's own rules, so they need no new semantics.
  - Bad: arity enforcement adds a comparison to each call of a strict function.
- **Alternatives considered:**
  - A pragma statement such as `strict()`: it runs as code, so it can't change how the chunk
    compiles.
  - A file extension such as `.flua`: it ties behavior to file names, which hosts that load
    scripts from strings don't have.
- **Revisit when:** the owner accepts or changes this record, which M4.6 needs first.

## ADR-0009 — Use one parser for the compiler and the tools

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** The compiler needs a fast parse of valid code. The formatter, linter, type checker,
  and language server need every comment and space, and a useful tree even for code that is still
  being typed. Two parsers would drift apart, and the tools would accept code the compiler rejects.
- **Decision:** `ferrolua-syntax` has one parser, which emits a stream of events: start a node,
  add a token, finish a node, report an error. Two consumers build from the same events:
  - a lossless syntax tree, which keeps every byte of the source and marks errors in place, for
    the tools;
  - a compact tree of the valid program, for the compiler.
  [`specs/syntax.md`](specs/syntax.md) gives the details.
- **Consequences:**
  - Good: the compiler and the tools agree on every program by construction.
  - Good: the parser recovers from errors once, for every tool.
  - Bad: the event stream adds a layer that a parser written only for the compiler wouldn't need.
- **Alternatives considered:**
  - A single-pass compiler like PUC-Rio's, plus a separate tool parser: faster to start, but two
    grammars to keep in agreement.
  - One lossless tree for everything: simpler, but `load` would build a heavy tree for every chunk
    at run time.
- **Revisit when:** benchmarks show that parsing through events makes `load` more than 20% slower
  than a direct parser would.

## ADR-0010 — Run Ferrolua's own register bytecode, and verify precompiled chunks

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** PUC-Rio Lua compiles source to a register-based bytecode and can save it as a
  precompiled chunk with `string.dump`. Its format changes between releases and fits its own VM.
  Ferrolua's VM, its reference counting (ADR-0018), and its extensions need instructions PUC-Rio's
  format lacks. A precompiled chunk is untrusted input: a crafted one can point a register or a jump
  anywhere.
- **Decision:** Ferrolua compiles to a register-based bytecode of its own, defined in
  `ferrolua-bytecode`. `string.dump` writes Ferrolua's format, and `load` reads only that format.
  Loading a precompiled chunk is off unless the host turns it on (ADR-0014), and a verifier checks
  every register, constant, upvalue, and jump target before the chunk runs. The JIT relies on the
  verifier's checks, so the verifier is part of the safety argument.
- **Consequences:**
  - Good: the bytecode fits the VM, and a crafted chunk is rejected before it runs.
  - Bad: chunks compiled by PUC-Rio's `luac` don't load in Ferrolua. This is an intended difference
    from PUC-Rio Lua 5.5.1.
- **Alternatives considered:**
  - PUC-Rio's format: it would load `luac` output, but the VM would inherit a format built for
    another design, and it changes with each release.
  - No precompiled chunks at all: safer still, but hosts that ship compiled scripts lose startup
    time.
- **Revisit when:** a host needs to load chunks compiled by PUC-Rio's `luac`.

## ADR-0011 — Keep the heap in safe Rust, with generational handles

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** A heap built from raw pointers needs `unsafe` code, and a reference-counting bug in
  it can read freed memory. ADR-0018 frees values by count and frees loops with a collector, so a
  bug that frees too early is the worst failure the heap can have.
- **Decision:** Each Lua state keeps its values in arenas: one growable array of slots for each
  kind of object, such as tables, strings, and closures. A reference to a value is a handle: the
  slot's index plus a generation number that changes each time the slot is reused. A stale handle,
  one whose generation doesn't match, is detected and raises an internal error instead of reading
  another value. The heap uses no `unsafe` code. [`specs/memory.md`](specs/memory.md) gives the
  details.
- **Consequences:**
  - Good: a counting bug can leak memory or raise an internal error, but never read freed memory.
  - Good: dropping the state drops the arenas, which frees everything left at once.
  - Good: the loop collector walks objects in slot order, which is the same on every run, so it
    frees and finalizes in a fixed order (ADR-0018).
  - Bad: each access checks an index and a generation, which costs time that raw pointers wouldn't.
    The JIT removes checks it can prove.
- **Alternatives considered:**
  - Raw pointers with `unsafe`: faster, but a counting bug becomes a memory-safety bug.
  - `Rc<RefCell<T>>` from the standard library: safe, but each value is its own allocation, and
    dropping the state doesn't free loops.
- **Revisit when:** benchmarks show that handle checks cost more than 10% of the interpreter's run
  time after the JIT removes the ones it can.

## ADR-0012 — Build a method JIT for x86-64 and AArch64 with assemblers of Ferrolua's own

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** A just-in-time (JIT) compiler turns hot bytecode into machine code while the program
  runs. There are two common designs:
  - A method JIT compiles whole functions.
  - A tracing JIT, like LuaJIT's, records the path a loop takes and compiles that path.
  Tracing JITs reach high speeds on loops, but they are harder to make correct and to debug. ADR-0002
  rules out assembler libraries, so Ferrolua writes its own encoders. ADR-0027 asks for a JIT on
  every platform that allows one: Linux, Windows, macOS, and Android, on x86-64 and AArch64.
- **Decision:** Ferrolua's JIT works in tiers:
  - The interpreter counts calls and loop iterations.
  - A hot function goes to a baseline tier, which translates each instruction into machine code,
    with guards on the types it has seen (M5).
  - A later optimizing tier builds a static single assignment (SSA) form of the function and
    optimizes it (M11).
  - When a guard fails, generated code exits to the interpreter at the same instruction, so the
    interpreter is always the reference.
  - Two backends: x86-64, with the System V calling convention on Linux, macOS, and Android and
    the Windows x64 convention on Windows; and AArch64, with the AArch64 procedure call standard
    and Apple's and Microsoft's variants of it. x86-64 on Linux comes first (M5), and the rest in
    M13.
  [`specs/jit.md`](specs/jit.md) gives the details.
- **Consequences:**
  - Good: each tier can be tested against the interpreter on the same scripts.
  - Good: a method JIT handles code without loops, such as callbacks, as well as loops.
  - Bad: each encoder is a large piece of work, which the overview sizes as L for x86-64, and the
    AArch64 backend repeats the baseline tier's templates for a second instruction set.
  - Bad: peak loop speed may stay below LuaJIT's.
- **Alternatives considered:**
  - A tracing JIT: faster on loops, harder to build correctly.
  - No JIT, only a fast interpreter: safer, but it misses the project's performance goal.
- **Revisit when:** benchmarks after M11 show the JIT missing its budget on loop-heavy scripts.

## ADR-0013 — Allow `unsafe` only in the JIT's memory and entry modules

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** The unsafe policy in `.claude/rules/rust.md` forbids `unsafe` in every crate until an
  accepted record names the crate, its modules, and the invariants its `unsafe` code relies on.
  Running generated machine code needs `unsafe`: Ferrolua must map memory, make it executable, and
  call into it. With the heap in safe Rust (ADR-0011), nothing else needs it.
- **Decision:** Only `ferrolua-jit` may use `unsafe`, and only in two modules:
  - `jit::memory` maps, protects, and unmaps executable memory with each operating system's own
    calls (ADR-0027: a JIT on Linux, Windows, macOS, and Android), declared in `extern` blocks
    against the libraries the standard library already links:
    - Linux and Android: `mmap`, `mprotect`, and `munmap`;
    - Windows: `VirtualAlloc`, `VirtualProtect`, `VirtualFree`, `FlushInstructionCache`, and
      `RtlAddFunctionTable` for the unwind tables of generated code;
    - macOS: `mmap` with `MAP_JIT`, `munmap`, `sys_icache_invalidate`, and, on AArch64,
      `pthread_jit_write_protect_np`.
    On Linux and Android for AArch64, it also flushes the instruction cache with the AArch64 cache
    maintenance instructions, through inline assembly, because no system call does.
  - `jit::entry` calls generated code.
  The code relies on these invariants:
  1. A page is writable or executable, never both (W^X). On macOS for AArch64, where `MAP_JIT`
     memory is both to the system, `pthread_jit_write_protect_np` keeps each thread on one side,
     and no thread runs code from memory it can write.
  2. Generated code runs only from a buffer that `jit::memory` finalized, and the buffer lives as
     long as any function or frame refers to it.
  3. Generated code reads and writes VM state only through `#[repr(C)]` layouts that `ferrolua-vm`
     exports, and tests check each layout's size and field offsets.
  4. Generated code checks each value's type and each index before using it, and exits to the
     interpreter when a check fails, unless the bytecode verifier (ADR-0010) already proved the
     fact.
  5. Generated code reloads every arena address after any operation that can allocate or free.
  6. At every exit to the interpreter, and at every safe point where the loop collector can run,
     reference counts match what the interpreter would hold (ADR-0018).
  7. On AArch64, every byte of a buffer reaches the instruction cache before the buffer first
     runs.
- **Consequences:**
  - Good: one crate, two modules, and seven invariants hold all of Ferrolua's `unsafe` code.
  - Bad: generated code itself can't be checked by the Rust compiler. Differential tests, which run
    each script interpreted and with the JIT forced on, and fuzzing stand in (M5.8).
  - Follow-up: `ferrolua-jit` repeats the workspace lints with `unsafe_code` at `deny`, and each
    `unsafe` block allows it locally, with its `// SAFETY:` comment.
- **Alternatives considered:**
  - Inline assembly system calls instead of `extern "C"`: no reliance on the C library, but more
    `unsafe` code to review.
  - No JIT: no `unsafe` at all, but no machine code either (ADR-0012).
- **Revisit when:** another crate needs `unsafe`, or a new platform needs other system calls.

## ADR-0014 — Make the VM the sandbox

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** Hosts run scripts they didn't write, such as game mods and user plugins. A script
  must not reach files, processes, or the host's memory unless the host allows it, and must not
  hang the host or exhaust its memory. `.claude/rules/lua.md` requires a record to settle this
  design before the standard library lands.
- **Decision:**
  - A host picks each state's libraries from an allowlist. `io`, `os`, `debug`, `package`,
    `string.dump`, and loading precompiled chunks are off unless the host turns them on.
  - The VM enforces limits on memory, instructions run, call depth, and the size of strings and
    tables. It counts work inside long library calls too, such as `string.rep` and `string.find`.
  - A limit stops the script and returns an error to the host, which `pcall` can't catch.
  - [`specs/sandbox.md`](specs/sandbox.md) gives the details.
- **Consequences:**
  - Good: a host can run untrusted code without an operating system sandbox, and a hostile script
    can only end with an error.
  - Bad: counting work inside library calls adds a cost to them.
  - Bad: a limit that `pcall` can't catch is a difference from PUC-Rio Lua, where running out of
    memory raises a catchable error. It applies only to limits the host sets.
- **Alternatives considered:**
  - Rely on the operating system, such as separate processes or seccomp: strong, but heavy for
    hosts that run many small scripts, and not portable.
  - Let `pcall` catch limit errors: a script could catch the error and keep running.
- **Revisit when:** the owner accepts or changes this record, which M3.2 needs first.

## ADR-0015 — Support editors through language and debug servers of Ferrolua's own

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** Auto complete and debugging happen in editors. The Language Server Protocol (LSP)
  and the Debug Adapter Protocol (DAP) let one server work with many editors, such as Neovim,
  Helix, Emacs, Zed, and Visual Studio Code. Both protocols exchange JSON messages. ADR-0002 rules
  out JSON and protocol crates.
- **Decision:** `ferrolua lsp` and `ferrolua dap` serve the two protocols over standard input and
  output. They are built on `ferrolua-json`, which parses and writes JSON and frames protocol
  messages. A host that embeds Ferrolua can also serve the debug protocol over a local TCP
  connection, so a developer can debug scripts inside a running host.
- **Consequences:**
  - Good: every editor with a built-in LSP or DAP client works with no plugin.
  - Bad: Visual Studio Code needs an extension to start a language server, and its usual client
    library is a third-party npm package, so a VS Code extension waits for a decision (see the
    roadmap's backlog).
- **Alternatives considered:**
  - Editor-specific plugins: one per editor, each in another language.
- **Revisit when:** the owner decides to publish a VS Code extension.

## ADR-0016 — Configure the tools with a data-only Lua file

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** The linter, formatter, type checker, and SAST tool (static application security
  testing, which finds security problems in source code without running it) need per-project
  settings. TOML, the format Cargo uses, would need a parser of Ferrolua's own (ADR-0002). Lua tools
  such as luacheck already use Lua files for settings, and Ferrolua has a Lua parser.
- **Decision:** The tools read `.ferrolua.lua` at the project root. Its one statement is
  `return { ... }`, built only from table constructors, strings, numbers, and booleans. The tools
  parse it and never run it, so it can't execute code.
  [`specs/diagnostics.md`](specs/diagnostics.md) lists the settings.
- **Consequences:**
  - Good: no second parser, and Lua users read the file as Lua.
  - Bad: the file can't compute values, such as a path built from an environment variable.
- **Alternatives considered:**
  - TOML: familiar from Cargo, but it needs its own parser.
  - A Lua file that runs in a sandbox: it could compute values, but running a configuration file
    is a security risk in a security tool.
- **Revisit when:** users ask for a computed setting that a static table can't express.

## ADR-0017 — Provide no Lua C API

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** PUC-Rio Lua is embedded in C programs through its C API (`lua.h`), and C modules for
  it are built against that API. Supporting it would mean `extern "C"` functions that take raw
  pointers from C code, which needs `unsafe`, and a stack-based interface that doesn't fit
  reference counting.
- **Decision:** Ferrolua has a Rust embedding API only, in the `ferrolua` crate. C modules built for
  PUC-Rio Lua don't load, `package.loadlib` returns an error, and `package.cpath` has no searcher.
- **Consequences:**
  - Good: no `unsafe` code at the host boundary, and an API that can hold Rust's guarantees.
  - Bad: C and C++ hosts can't embed Ferrolua, and Lua libraries written in C, such as LuaSocket and
    LuaFileSystem, don't load.
- **Alternatives considered:**
  - The full Lua C API: the whole C ecosystem, at the cost of `unsafe` code at every call.
- **Revisit when:** a C or C++ host asks to embed Ferrolua.

## ADR-0018 — Free loops with a deterministic collector on top of reference counting

- **Status:** Accepted; ADR-0021 adds that bytes count by fixed charges, alike on every target,
  and that a collection's finalizers run in the call that ran it
- **Date:** 2026-11-24
- **Context:** ADR-0003 freed memory by reference counting alone and left reference loops in memory
  until their state was dropped. Lua code makes loops often: `Class.__index = Class`, trees with
  parent pointers, doubly linked lists, and coroutines that refer to themselves. A long-running
  host, such as a game or a server that keeps one state for hours, would grow without bound. On
  2026-11-24 the owner asked for a deterministic garbage collector instead. Here, deterministic
  means the collector runs at points the program itself fixes, never on a timer or on another
  thread, so the same run frees the same values in the same order every time. That matters for
  tests, for replays, and for games that keep several machines in lockstep.
- **Decision** (the owner chose reference counting plus a loop collector, run by allocation count
  and on request, the first of two options for each, on 2026-11-24):
  - Every value is still reference counted, and is freed as soon as its count reaches zero.
  - A loop collector frees the values that only loops keep alive. It uses trial deletion, the
    synchronous cycle collection of Bacon and Rajan's "Concurrent Cycle Collection in Reference
    Counted Systems" (2001). A value whose count drops to a number above zero may now be part of an
    unreachable loop, so it becomes a candidate. The collector subtracts the references among the
    candidates and everything they reach, restores whatever something outside still points to, and
    frees the rest.
  - The collector needs no list of roots: registers, the registry, and values the host holds all
    keep counts, so whatever they reach survives.
  - It runs only at safe points, at two kinds of time: when the bytes allocated since its last run
    pass a threshold the host sets, and when a script calls `collectgarbage()` or the host calls
    `Lua::collect`. `collectgarbage("stop")` turns the automatic runs off.
  - Given the same script, inputs, host calls, options, and hash seed, every collection runs after
    the same instruction, and frees and finalizes the same values in the same order. This holds
    interpreted and with the JIT, because only Lua heap bytes count toward the threshold.
  - A local function's references to itself still compile to "the running function", and classes
    (ADR-0007) still need no self-reference, so the commonest loops never reach the collector.
  - [`specs/memory.md`](specs/memory.md) gives the details.
- **Consequences:**
  - Good: loops are freed, so long-running hosts stay within a steady amount of memory.
  - Good: `collectgarbage()` frees what scripts expect it to free, so fewer scripts differ from
    PUC-Rio Lua 5.5.1, and weak tables, ephemerons included, behave as §2.5.4 of the manual says.
  - Good: most values are still freed the moment their last reference goes, and every free happens
    at a point the program fixes.
  - Good: the heap stays safe Rust (ADR-0011), and the JIT needs no write barriers, the extra code a
    tracing collector adds to every store.
  - Bad: a collection pauses the script for time that grows with the number of candidates and the
    values they reach. A host that can't accept that turns automatic runs off and collects between
    frames.
  - Bad: every decrement that leaves a count above zero records a candidate, on top of the cost of
    counting (M5.1 measures both).
  - Bad: values outside loops are still freed earlier than PUC-Rio frees them, so a script that
    observes `__gc` or weak tables without calling `collectgarbage()` can differ; those scripts go
    in `tests/extensions/`.
  - Follow-up: M2.17 builds the collector; M3.1 adds `Lua::collect` and the collector's options;
    M3.2 runs a collection before a memory limit stops a script; M5.8 checks that the JIT
    collects at the same points as the interpreter.
- **Alternatives considered:**
  - A tracing collector with no reference counting, like PUC-Rio's, scheduled by allocation count:
    the closest match to PUC-Rio's `__gc` timing, but nothing is freed the moment its last reference
    goes, and the VM and the JIT need write barriers.
  - A collector that runs only on request: the host chooses every pause, but loops grow until it
    asks.
  - Reference counting alone (ADR-0003): no pauses, but loops stay until the state is dropped.
- **Revisit when:** a host measures a collection pause above the budget in
  [`00-overview.md`](00-overview.md#budgets) that scheduling collections can't avoid, or M5.1 shows
  counting and candidate tracking together cost more than a third of the interpreter's run time.

## ADR-0019 — Print object IDs, never memory addresses

- **Status:** Accepted
- **Date:** 2026-11-24
- **Context:** PUC-Rio Lua prints an object's memory address wherever it names an object:
  `tostring({})` gives something like `table: 0x5581f1a3c2a0`, and `string.format("%p", t)` and
  `debug.upvalueid` expose addresses too. An address tells a script where the host's memory is.
  That weakens address space layout randomization (ASLR), the operating system's defense of placing
  memory at unpredictable addresses, and helps an attacker exploit any memory bug in the host.
  Addresses also change from run to run, so output that contains one can't repeat, which breaks the
  determinism ADR-0018 promises and keeps such output out of tests. Ferrolua's values are handles
  into arenas, not pointers (ADR-0011, proposed), so it has no reason to print addresses at all.
- **Decision** (the owner chose IDs instead of addresses on 2026-11-24):
  - Every object that Lua code can name, such as a table, a function, a thread, a userdata, or a
    string, gets a per-state ID: a positive integer from a counter that starts at 1, assigned the
    first time something asks for the object's ID.
  - An ID is never reused in its state, even after its object is freed.
  - `tostring` and `print` show the ID where PUC-Rio shows an address, as in `table: #42`.
    `string.format("%p")` prints the ID, and `debug.upvalueid` returns an upvalue's ID as an
    integer. The debugger shows the same IDs.
  - No output Ferrolua produces contains a memory address.
  - [`specs/memory.md`](specs/memory.md#object-ids) says how IDs are assigned, and
    [`specs/stdlib.md`](specs/stdlib.md#object-ids-in-output) gives the exact formats.
- **Consequences:**
  - Good: a script learns nothing about the host's memory layout from anything it prints or
    computes.
  - Good: with a fixed hash seed, output that names objects repeats exactly, interpreted and
    compiled, so tests can check it.
  - Good: two objects never print alike within a state, while PUC-Rio can reuse an address after
    collecting the first object.
  - Bad: this is an intended difference from PUC-Rio Lua 5.5.1. Code that parses hexadecimal out of
    `tostring`'s result, or expects `debug.upvalueid` to return a light userdata, behaves
    differently.
  - Bad: each object that has been given an ID costs an entry in a side table until it is freed.
  - Follow-up: M2.13 assigns IDs and prints them from `tostring`, M3.4 from `%p`, M3.9 for file
    handles, M3.11 from `debug.upvalueid`, and M10.3 in the debugger.
- **Alternatives considered:**
  - Print addresses, as PUC-Rio does: compatible, but it leaks the memory layout and never repeats.
  - Print the handle's slot index and generation: no address, but it reveals how the heap is laid
    out, and the same slot prints alike once its generation wraps.
  - Print the ID in hexadecimal with a `0x` prefix: code that parses addresses keeps working, but
    the output would pass for an address and mislead readers.
- **Revisit when:** a widely used Lua library breaks on Ferrolua because it parses `tostring`'s
  output.

## ADR-0020 — Bound the probe steps of table operations, and hash reference keys by handle

- **Status:** Accepted; on 2026-11-24 the owner chose counting over a cap for the bound, which
  [`specs/vm.md`](specs/vm.md#tables) records
- **Date:** 2026-11-24
- **Context:**
  - A table's hash part uses open addressing on the state's hash seed
    ([`specs/vm.md`](specs/vm.md#tables)). Keys chosen to land on the same slots make each lookup
    walk a long chain, checking one slot per probe step, so inserting n of them takes on the order
    of n² probe steps. This attack is called hash flooding.
  - By default the seed comes from the standard library's `RandomState`, so a script can't predict
    collisions ([`specs/sandbox.md`](specs/sandbox.md#isolation)). A host that needs runs to repeat
    fixes the seed with `Options::hash_seed` ([`specs/ferrolua.md`](specs/ferrolua.md)), and then
    the defense is gone: under a seed known in advance, a script, or data that a script uses as
    keys, can make one table operation run for seconds.
  - The instruction limit counts bytecode instructions and the work units inside library calls, but
    not the probe steps inside one instruction ([`specs/sandbox.md`](specs/sandbox.md#limits)), so
    it never sees that stall.
  - The specs don't say how a table, a function, a thread, or a userdata used as a key is hashed.
    PUC-Rio Lua hashes them by address, so the order in which `pairs` visits them changes between
    runs whatever the seed. ADR-0018 and ADR-0019 promise runs that repeat exactly under a fixed
    hash seed.
  - [Nadir](https://github.com/jabbott-iii/Nadir), a game engine that embeds Ferrolua, needs both.
    Its ADR-0080 sets each state's seed from the game's seed, so that recordings replay, and asks
    Ferrolua to bound probe steps and to hash reference keys by handle.
- **Decision** (the owner chose both points on 2026-11-24, with Nadir's ADR-0080):
  - **Bounded probes.** No set of keys, under any seed, makes one table operation run long. Either
    each probe step past the first counts as one work unit against the instruction limit, as
    library work already does, or a probe's steps are capped, with a fallback that keeps lookups
    bounded when keys share a full hash. M2.4 decides which before work starts, and records it in
    [`specs/vm.md`](specs/vm.md#tables).
  - **Reference keys hashed by handle.** A table, a function, a thread, or a userdata used as a key
    is hashed from its handle (its slot and generation, [`specs/memory.md`](specs/memory.md)) mixed
    with the state's seed, never from an address. Slots are reused in a fixed order (ADR-0018), so
    the same script, inputs, host calls, options, and seed give the same handles and the same
    `pairs` order.
- **Consequences:**
  - Good: under a fixed seed, `pairs` visits every kind of key in the same order in every run, so a
    host that replays runs, or keeps machines in lockstep, can use `Options::hash_seed`.
  - Good: a host that fixes the seed doesn't open itself to hash flooding.
  - Bad: counting gives no bound to a host that sets no instruction limit, and stops a script that
    PUC-Rio would run slowly to its end; a cap needs a fallback structure, more memory, and more
    code in every table operation.
  - Bad: the order of reference keys depends on how the state reused its slots, so a script that
    allocates differently gets another order, as strings do under another seed.
  - Follow-up: M2.3 takes a seed from its caller; M2.4 hashes reference keys by handle, builds the
    bound, and decides between counting and a cap; M3.1 passes `Options::hash_seed` through; M3.2
    tests the bound against a crafted set of keys if M2.4 chose counting.
- **Alternatives considered:**
  - Leave probes unbounded and rely on the random seed: a host that fixes the seed loses the
    defense.
  - Hash reference keys by their object IDs (ADR-0019): stable, but every key would need an ID and
    its entry in the side table.
  - Tables that keep their keys in the order they were added, as Python's dictionaries do: a stable
    order whatever the seed, but a larger redesign of tables. Nadir chose a fixed seed instead.
- **Revisit when:** a host reports a table operation that runs long under a fixed seed, or M5.1's
  benchmarks show that the bound slows table lookups.

## ADR-0021 — Count instructions per call, finalizers included, and bytes alike on all targets

- **Status:** Accepted; on 2026-11-24 the owner accepted the agent's proposal for meeting both
  requirements
- **Date:** 2026-11-24
- **Context:**
  - [Nadir](https://github.com/jabbott-iii/Nadir), a game engine that embeds Ferrolua, keeps
    finalizers and weak tables in its scripts (its ADR-0081, which the owner accepted on
    2026-11-24). It turns the loop collector's automatic runs off and calls `Lua::collect` at the
    end of each game tick, after every script has run, so each loop's finalizer runs at one known
    point. It asks two things of Ferrolua:
    - the finalizers that `Lua::collect` runs count against that call's instruction budget, so one
      that loops forever stops there and costs no other script;
    - a state's allocated bytes count the same on all six of Nadir's targets (x86-64 and AArch64,
      on Windows, macOS, and Linux), so the memory limit, and the collection that runs before it
      stops a script, fall at the same point on each. Nadir checks that a recorded game replays to
      the same state on Linux, Windows, and macOS.
  - [`specs/sandbox.md`](specs/sandbox.md#limits) doesn't say whether the instruction limit counts
    each call from the host or the state's whole life. Nadir's ADR-0079 asks for a budget for each
    call (its requirement 5).
  - Finalizers run at the safe points of whatever call is running
    ([`specs/memory.md`](specs/memory.md#finalizers)). If a limit stopped `Lua::collect` partway
    through the finalizers its collection queued, the rest would run in the host's next call, in a
    script that has nothing to do with them.
  - The memory spec counts every allocation the state makes, but promises the same collections only
    on the same target. Sizes taken from Rust can differ by target: the standard library's
    `HashMap` allocates a few more bytes on x86-64, where it scans its control bytes 16 at a time
    with SSE2, than on AArch64, where it scans 8; Rust promises no layout for its own types; and
    how far a `Vec` grows is a detail of the standard library that can change between releases.
- **Decision** (the owner chose the two requirements on 2026-11-24, with Nadir's ADR-0081, and
  accepted the agent's proposal below for meeting them the same day):
  - **The instruction limit counts per call.** Its count starts at zero each time the host calls
    into a state that is running nothing: `exec`, `eval`, a call of a Lua function or a resume from
    Rust, and `Lua::collect`. Everything the call runs counts against it, including the finalizers
    that run during it and the Lua code that the host's Rust functions call in turn. The host can
    change the limits between calls, so a collection can have a budget other than its scripts'. The
    memory limit still counts the bytes the state holds, across calls.
  - **A collection's finalizers run in the call that ran it,** whether `Lua::collect`,
    `collectgarbage`, the schedule by allocation count, or the run before a memory error started
    it. If a limit stops that call first, the rest stay queued in their order and run at the start
    of the next collection, before its own. They never run at another call's safe point, and
    `Lua::collect` returns the limit error.
  - **Bytes count by fixed charges.** The memory spec sets a charge for each kind of object and for
    each element of each kind of buffer, such as 16 bytes for each slot of a table's array part.
    The count never comes from `size_of`, `Vec::capacity`, or the allocator. Ferrolua sizes every
    buffer by its own growth rules and reserves exactly that capacity, and the tables that index
    objects, such as the string intern table and the object-ID side table, charge a fixed amount
    for each entry. The same allocations therefore count the same bytes on every target and with
    every allocator, so the schedule by allocation count, the memory limit, the collection before
    it, and `collectgarbage("count")` fall at the same points everywhere.
  - **Stored collection logs.** The collection log records the counted bytes at each collection,
    and the conformance runner compares each script's log with the one stored for it, so a target
    that counts differently fails.
- **Consequences:**
  - Good: a host that collects at a point of its choosing knows where every loop's finalizer runs
    and what it may cost, and a finalizer that loops forever stops at that collection's limit.
  - Good: with a fixed hash seed, a run repeats on every target, not only on one, which hosts that
    replay runs or keep machines in lockstep need.
  - Good: a host bounds each call without tracking what earlier calls used.
  - Bad: no limit covers a state's total work across calls; a host that wants one keeps its own
    total.
  - Bad: the finalizers a stopped collection left wait for the next collection, and hold their
    objects' memory until then; with automatic runs off, that is the host's next `Lua::collect`.
  - Bad: the count models the memory rather than measuring it. It leaves out the allocator's own
    overhead and the free slots an arena keeps for reuse, and each charge must stay no smaller than
    the type it stands for, which a test checks.
  - Bad: every change to what Ferrolua allocates, or to a charge, changes the stored logs, which
    are recorded again and reviewed in the same change, and a host's recordings from an earlier
    release may diverge.
  - Follow-up: M2.2 builds the charges and records them in the memory spec; M2.17 stores the
    collection logs and compares them; M3.1 makes `Lua::collect` a call like any other and lets the
    host change the limits between calls; M3.2 counts the instruction limit per call, keeps a
    stopped collection's remaining finalizers for the next one, and decides under what limit the
    finalizers that run when a state is dropped run; each target that joins
    [`03-platforms.md`](03-platforms.md) compares the stored logs.
- **Alternatives considered:**
  - One limit for the state's whole life, which the host raises before each call: the host must
    know what each call used, and a collection's finalizers share whatever budget the calls before
    them left.
  - A stopped collection's remaining finalizers run at the next safe point, as other pending
    finalizers do: they would run, and be charged, in a call that has nothing to do with them.
  - Count what the allocator returns, or `size_of` and `Vec::capacity`: closer to the real memory,
    but it differs by target, allocator, and Rust release, so runs would repeat on one target only.
  - Promise the same count on one target only, as the memory spec did: each of Nadir's targets
    would collect at its own points, and its recordings wouldn't replay on the others.
- **Revisit when:** a host measures a state whose real memory is more than twice its counted bytes,
  or needs a limit on a state's total work across calls.

## ADR-0022 — Put the JIT behind an optional `jit` feature of `ferrolua`, off by default

- **Status:** Accepted; on 2026-11-24 the owner chose the agent's recommendation, an optional
  feature off by default. Keeping `JitMode` in both builds, refused without the feature, is the
  agent's detail within it
- **Date:** 2026-11-24
- **Context:**
  - The `ferrolua` crate, the one a host adds, depends on `ferrolua-jit`
    ([`01-architecture.md`](01-architecture.md#crate-map)). `ferrolua-jit` is the only crate with
    `unsafe` code (ADR-0013, proposed: only its memory and entry modules), maps executable memory,
    and has a backend for Linux on x86-64 only (ADR-0012, proposed: a method JIT with tiers).
    `ferrolua-vm` already doesn't depend on it: the VM defines a `Tier` trait for compiled code, and
    `ferrolua` plugs the JIT in.
  - Every host therefore links the JIT's code, even one that sets `JitMode::Off`.
    [Nadir](https://github.com/jabbott-iii/Nadir), a game engine that embeds Ferrolua, takes a
    Ferrolua crate as its own code only if the crate forbids `unsafe`, and asks that the JIT be a
    crate of its own or a feature off by default (its ADR-0079, requirement 10). Its dependency
    check fails on any package in its `Cargo.lock` that it doesn't allow, so as planned it can't
    depend on `ferrolua`.
  - Cargo leaves an optional dependency that no crate turns on out of a host's `Cargo.lock`, but
    lists a dependency gated to one target on every machine. Inside a workspace, it turns a feature
    on for every crate built in the same run once one crate asks for it: `cargo test --workspace`
    builds a crate with a feature that a sibling turns on, and `cargo test -p <crate>` builds it
    without. Both were checked on 2026-11-24 in a scratch workspace.
- **Decision** (the owner chose an optional feature, off by default, on 2026-11-24, over a run-time
  switch, a crate the host adds, a dependency gated by target, and hosts using the inner crates):
  - **An optional feature.** `ferrolua-jit` is an optional dependency of `ferrolua`, turned on by
    its `jit` feature (`jit = ["dep:ferrolua-jit"]`), which is off by default. Without it, a host's
    build contains no `ferrolua-jit`, and no Ferrolua code with `unsafe`.
  - **The same API in both builds.** `JitMode` and `Options::jit` exist with or without the
    feature, so turning it on, as any crate in a host's tree can, changes no host's code. Without
    the feature, the default mode is `JitMode::Off`, and `Lua::new` refuses `JitMode::On` and
    `JitMode::Forced` with an error that names the feature, so a host that asks for the JIT without
    it finds out when it creates the state.
  - **Who turns it on.** `ferrolua-cli`, so the `ferrolua` command runs with the JIT, and any host
    that asks with `features = ["jit"]`. The conformance runner's forced pass and the
    `differential` fuzz target need it too.
  - **Both builds checked.** CI lints, tests, and covers `ferrolua` with the feature on, in the
    workspace run, and off, with `-p ferrolua`, and checks that
    `cargo tree -p ferrolua --edges normal` lists no `ferrolua-jit`.
- **Consequences:**
  - Good: a host gets no `unsafe` code and no executable memory unless it asks, as the
    architecture's first principle, memory safety first, wants. Nadir's requirement 10 is met.
  - Good: without the feature, `ferrolua` is portable Rust, so a new target needs CI jobs for the
    interpreter, not a JIT backend.
  - Good: the `Tier` trait stays between `ferrolua-vm` and `ferrolua`, and the embedding API is the
    same in both builds.
  - Bad: a host that adds `ferrolua` and reads nothing else runs at the interpreter's speed, not
    the JIT's.
  - Bad: asking for the JIT without the feature fails when the state is created, not when the host
    compiles.
  - Bad: `ferrolua` has two builds to lint, test, and cover, and its code that plugs the JIT in sits
    behind `#[cfg(feature = "jit")]`.
  - Follow-up: M5.4 adds the feature, `JitMode` in both builds, and the checks of both builds;
    M5.5 to M5.7 run their forced-mode checks with it on; M5.8 turns the feature on in
    `ferrolua-cli` and the fuzzer.
- **Alternatives considered:**
  - Keep the dependency, and let hosts turn the JIT off with `JitMode::Off`: every host still links
    the JIT's code, and Nadir's dependency check rejects it.
  - The same feature, on by default: most hosts get the JIT without asking, and Nadir could still
    leave it out with `default-features = false`, but every other host gets `unsafe` code by
    default.
  - A crate the host adds and passes to `Options`: no feature, but the `Tier` trait becomes a public
    interface of `ferrolua-vm`, and hosts keep two crates at matching versions.
  - A dependency for Linux on x86-64 only: Cargo still lists it in every host's `Cargo.lock`, and
    hosts on that target link it.
  - Hosts use `ferrolua-vm`, `ferrolua-compiler`, and `ferrolua-stdlib` directly: those APIs are
    internal, and each host would rewrite what `ferrolua` wires together.
- **Revisit when:** a host reports running without the JIT because it missed the feature, or the
  `Tier` trait has to become public for another reason.

## ADR-0023 — Make Ferrolua a language of its own, with Python's operators and 0-based indexes

- **Status:** Accepted; ADR-0025 adds f-strings, locals by default, lists, tuples, sets, and
  Python's truth
- **Date:** 2026-11-24
- **Context:**
  - ADR-0004 made Ferrolua's language a superset of Lua 5.5, so every valid Lua 5.5 script runs
    unchanged, and ADR-0005 added `++` and compound assignment as C, Java, and JavaScript write
    them. On 2026-11-24 the owner asked instead for operators that follow Python's, for simplicity,
    and for indexes that start at 0 instead of 1.
  - Neither change fits a superset, which can add only syntax that is an error in Lua 5.5 today.
    Lua already gives `^` (power), `..` (concatenation), `~=` (not equal), and binary `~` (XOR)
    meanings of their own, and a Lua script that reads `t[1]` expects a list's first element.
  - Asked the same day how far the change should go, the owner chose a language of its own over two
    other options: a mode that each chunk turns on, and Python's spellings only where Lua leaves
    them unused, such as `!=` and `**`, which would keep indexes 1-based.
  - The owner then chose, for each detail that choice raised: `+` joins strings, and no operator
    converts between strings and numbers; `#` stays the length operator; Python's `in`, `not in`,
    `is`, `is not`, chained comparisons, and conditional expressions join the language; a range
    excludes its end, as Python's `range` does; operators on numbers give Python's results; and
    library functions stop converting their arguments too.
  - PUC-Rio Lua 5.5.1 is the reference for what Ferrolua shares with Lua 5.5 (ADR-0001), and 26
    documents relied on running the same script on both.
- **Decision** (the owner chose the first of three options for the scope, and one option for each
  of six details, on 2026-11-24):
  - **A language of its own.** Ferrolua's language derives from Lua 5.5. It keeps Lua's statements,
    values, tables, metatables, closures, coroutines, and standard library, apart from the changes
    below. It no longer promises to run Lua 5.5 scripts unchanged, and has no mode that does.
    Strict mode stays off unless a chunk or its host turns it on, as ADR-0004 decided.
  - **Python's operators.** Operators take Python's spellings, precedence, and associativity. `**`
    raises to a power, `!=` tests inequality, `^` is bitwise XOR, and `+` also joins two strings.
    `not` binds more loosely than a comparison, so `not a == b` means `not (a == b)`. Lua's `..`,
    `~=`, and binary `~` are gone, and with `..` its metamethod `__concat`; unary `~` stays bitwise
    NOT, and `#` stays the length operator.
  - **Python's additions.** `k in t` is true when table `t` has the key `k`, or when string `t`
    contains the string `k`, and `not in` is its negation. `a is b` is true when both sides are the
    same object, as `rawequal(a, b)` tests, and `is not` is its negation. Comparisons chain:
    `a < b < c` means `a < b and b < c`, with `b` evaluated once. `x if c else y` is a conditional
    expression.
  - **Compound assignment as in Python.** `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `|=`,
    `^=`, `<<=`, and `>>=` stay statements with one target, as ADR-0005 made them. `++` and `..=`
    are gone.
  - **No implicit conversion.** Neither an operator nor a library function converts between strings
    and numbers: `"10" + 1` and `string.rep("x", "3")` raise errors. `tostring` and `tonumber`
    convert.
  - **Python's results on numbers.** Integers stay 64-bit and wrap around on overflow, as in
    Lua 5.5. An integer raised to a non-negative integer power gives an integer, `>>` keeps the
    sign, a negative shift count raises an error, and division by zero with `/`, `//`, or `%` raises
    an error, for integers and floats alike.
  - **Indexes from 0, and ranges without their end.** Table constructors, `#`, `ipairs`, `select`,
    varargs, and every library position count from 0, and a negative position counts back from the
    end, with -1 the last. A range covers its start but not its end: `for i = 0, n do` runs n times,
    and `string.sub(s, 0, 3)` gives the first three bytes.
  - [`specs/language.md`](specs/language.md) gives the grammar and the meanings, and
    [`specs/stdlib.md`](specs/stdlib.md) the positions each library function takes. ADR-0024
    settles the details these choices leave open.
- **Consequences:**
  - Good: a reader who knows Python reads Ferrolua's expressions, indexes, and ranges without
    learning Lua's spellings, and a loop over a list needs no `- 1`.
  - Good: `"10" + 1` raises an error instead of giving 11, so a string that holds a number can't
    change a sum unnoticed.
  - Good: `x if c else y` gives `x` even when `x` is `false` or `nil`, where Lua's idiom
    `c and x or y` gives `y`.
  - Bad: a Lua 5.5 script runs unchanged only if it uses none of the changed operators, indexes, or
    ranges. Existing Lua code and libraries need porting, and Lua tools such as lua-language-server,
    StyLua, and luacheck misread Ferrolua files.
  - Bad: `in` tests a table's keys, as Python's `in` does for a dictionary, so `3 in {1, 2, 3}` is
    false: that table's keys are 0, 1, and 2. A Python programmer who reads the table as a list
    expects true.
  - Bad: PUC-Rio Lua can't run Ferrolua's conformance scripts. Each becomes a pair: a Lua 5.5
    reference script that PUC-Rio runs to record the expected output, and its Ferrolua twin, which
    must print the same ([`04-quality.md`](04-quality.md#conformance-scripts)). Behavior with no
    Lua 5.5 counterpart is checked against output written from the spec.
  - Bad: code ported from Lua that squares with `x ^ 2` now computes an XOR, with no error when `x`
    is an integer. A porting rule of the linter reports it (M7.2).
  - Bad: the `if` of a conditional expression can follow an expression at a place where an `if`
    statement can also start, so the parser decides by the token after the condition
    ([`specs/language.md`](specs/language.md#conditional-expressions)).
  - Follow-up: M1.2 and M1.5 lex and parse the new operators; M2.4 numbers tables from 0; M2.6 and
    M2.7 compile chained comparisons, conditional expressions, and half-open `for` loops; M2.9
    builds the operators' results and records their messages; M2.13 and M3.3–M3.11 move the
    libraries to 0-based, half-open positions with no conversion; M4.1 and M4.2 build Python's
    compound assignment; M7.2 adds the porting rules; the owner decides whether Ferrolua files keep
    the `.lua` extension.
- **Alternatives considered:**
  - A mode that each chunk turns on, with Lua 5.5 as the default: existing scripts keep running,
    but every table passed between the two modes needs its integer keys shifted, in the VM, the JIT,
    and every library function.
  - Python's spellings only where Lua leaves them unused, such as `!=` and `**`: the superset holds,
    but indexes stay 1-based, `^` stays power, and `..` stays.
- **Revisit when:** a host needs to run Lua 5.5 code it can't port, such as a library it doesn't
  maintain.

## ADR-0024 — Settle what Python's operators and 0-based indexes leave open

- **Status:** Accepted; ADR-0025 replaces its rule for truth with Python's
- **Date:** 2026-11-24
- **Context:** ADR-0023 records the owner's choices: Python's operators, indexes from 0, ranges
  without their end, and no implicit conversion. Writing them into the language spec raised
  questions those choices don't answer. Python and Lua disagree about which values are false.
  Python's `in` works through a method that Lua's values don't have. Python never accepts a float
  where it needs an integer, while Lua 5.5 accepts `3.0`. Some numbers in Lua count things that
  aren't positions, such as lines and stack levels. The agent proposed these answers on
  2026-11-24, and the owner accepted them the same day.
- **Decision** (the owner accepted the agent's proposal on 2026-11-24):
  - **Truth stays Lua's.** Only `nil` and `false` are false. `0`, `""`, and an empty table are
    true, unlike in Python.
  - **Membership.** `k in v` calls `v`'s `__contains` metamethod, a new one, when `v` has it, and
    converts the result to a boolean. Otherwise it is true when `v` is a table and `rawget(v, k)`
    isn't `nil`, so `__index` isn't consulted, or when `v` and `k` are both strings and `k` occurs
    in `v` as plain text, not as a pattern. Any other pair raises an error.
  - **`is` is a reserved word,** so `a is b` can't also read as `a` followed by a statement that
    starts with a variable named `is`.
  - **Unary `+`** returns a number unchanged and raises an error for any other value. It has no
    metamethod.
  - **No float stands in for an integer.** A bitwise operand, a shift count, and a library
    parameter that takes an integer reject a float, even one with an integer value such as `3.0`,
    as Python does; `math.tointeger` and `math.floor` convert. A float key with an integer value,
    such as `2.0`, still names the same field as `2`, as in Lua and in Python's dictionaries.
  - **What still counts from 1:** line and column numbers; argument numbers in messages, such as
    `bad argument #1`; stack levels in `error` and `debug.getinfo`; pattern capture numbers, where
    `%1` is the first capture and `%0` the whole match, as Python's `re` numbers its groups; and the
    month, day, and weekday fields of `os.date` and `os.time`, which are calendar values, not
    positions.
  - **`arg` keeps Lua's layout,** which Python's `sys.argv` shares: `arg[0]` is the script, and
    `arg[1]` the first argument.
  - **`debug` numbers locals and upvalues from 0,** in `getlocal`, `setlocal`, `getupvalue`,
    `setupvalue`, `upvalueid`, and `upvaluejoin`. A negative number still names a vararg, with -1
    the first.
  - **`math.random(0)`** still returns an integer whose bits are all random, because the empty
    range it would otherwise name has no use.
  - **`ferrolua run` reads `FERROLUA_INIT`** instead of `LUA_INIT_5_5` and `LUA_INIT`, which hold
    Lua 5.5 code for `lua`.
  - **Not adopted from Python:** `@` (matrix multiplication), `:=` (assignment inside an
    expression), slices such as `t[0:3]`, `lambda`, and `await`. Ferrolua has no matrices, its
    assignments are statements, `:` already calls methods, `function` already makes an anonymous
    function, and coroutines already suspend.
- **Consequences:**
  - Good: ported Lua code keeps every `if`, `while`, `and`, and `or` working as before.
  - Good: `in` works on tables, strings, and a host's userdata, and needs no `__index` call that
    could raise an error, such as a class instance's for an undeclared field.
  - Bad: a Python programmer expects `0` and `""` to be false, but `if count then` runs its block
    when `count` is 0.
  - Bad: `string.rep("x", n / 2)` raises an error, because `/` gives a float; ported code writes
    `n // 2`.
  - Bad: a Lua script that names a variable `is` must rename it.
  - Follow-up: M1.5 reserves `is`; M2.9 builds `in`, `is`, unary `+`, and the integer checks of the
    bitwise operators; M2.13 rejects floats for integer parameters; M3.7 keeps `math.random(0)`;
    M3.11 numbers locals and upvalues from 0; M3.14 reads `FERROLUA_INIT`.
- **Alternatives considered:**
  - Python's truth, where `0`, `""`, and empty containers are false: familiar to Python
    programmers, but it changes every condition in ported code, and a table would be false when
    empty whether it serves as a list or a dictionary.
  - `in` that follows `__index`: it would see inherited fields, but it would also run code, and
    raise an error for a class instance's undeclared field.
  - `is` as a contextual keyword: old names keep working, but `a is (b)` would read both as an
    identity test and as `a` followed by the call `is(b)`.
  - Floats with integer values accepted as integers, as in Lua 5.5: fewer porting changes, but
    `3.0 | 1` would work where Python raises an error, against the owner's choice of Python's
    results.
  - Reading `LUA_INIT`, as `lua` does: a user's Lua 5.5 startup code would fail to parse.
- **Revisit when:** users report bugs from expecting `0` or `""` to be false, as in Python, or a
  host needs `in` to see fields that `__index` provides.

## ADR-0025 — Add f-strings, function-wide locals, lists, tuples, and sets, and Python's truth

- **Status:** Accepted; the owner also accepted ADR-0026, which settles its details, on 2026-11-24
- **Date:** 2026-11-24
- **Context:**
  - ADR-0023 made Ferrolua a language of its own, with Python's operators and 0-based indexes, and
    ADR-0024 kept Lua's rule for truth, where only `nil` and `false` are false. On 2026-11-24 the
    owner asked for four more of Python's features: formatted string literals (f-strings),
    local-by-default scoping, dedicated collection types, and Python's truth.
  - Lua makes a name global when code assigns it without declaring it, so a typo in an assignment
    creates a global that every chunk sees. Lua 5.5's `global` declarations can turn that off, but
    only by declaring every global name a chunk uses.
  - Lua has one structured type, the table, which serves as list, dictionary, record, object,
    module, and metatable. A table can't hold `nil` as a list element, its length is unclear when
    it has holes, it can't be a key by value, and nothing marks one as a set.
  - Asked the same day, the owner chose lists, tuples, and sets beside tables over two other
    options: Python's four types in place of tables, and lists and sets only. For scoping, the
    owner chose locals that reach the whole function, as in Python, over locals that reach the
    rest of their block, as Lua's `local` does.
- **Decision** (the owner chose the four features, the first of three options for collections, and
  the first of two for scoping, on 2026-11-24):
  - **f-strings.** `f"..."` is a formatted string literal, as in Python:
    `f"{name} has {n:>3} items"` evaluates each field in braces, formats it, and joins the parts
    into one string. A field takes Python's conversions `!s` and `!r`, the `=` that writes the
    expression's text before its value, and Python's format specification mini-language after `:`.
  - **Locals by default, for the whole function.** Assigning a name anywhere in a function makes it
    a local of that whole function, unless the function declares it `global` or `nonlocal`, as in
    Python. Reading such a local before any assignment raises an error, and no assignment creates
    a global unless a `global` declaration says so. `local` stays, for a local that lasts to the
    end of its block and can take `<const>` or `<close>`. Reading a name that no scope declares
    reads a global, as in Lua and Python.
  - **Lists, tuples, and sets beside tables.** `[1, 2]` makes a list, `(1, 2)` a tuple, and
    `set(...)` a set, each a type of its own. A list is a sequence that can change, a tuple one
    that can't, and a set holds distinct values in no order. `in` tests a list's or a tuple's
    values, so `3 in [1, 2, 3]` is true. Tables stay, as dictionaries, records, modules, and
    metatables.
  - **Python's truth.** `nil`, `false`, `0`, `0.0`, `""`, and empty collections are false, and other
    values are true, as in Python. This replaces ADR-0024's rule.
  - [`specs/language.md`](specs/language.md) gives the syntax and the meanings. ADR-0026 (proposed)
    settles the details these choices leave open.
- **Consequences:**
  - Good: `f"{x} of {n}"` replaces `string.format("%s of %d", x, n)`, and the `tostring` calls that
    `+` needs, since `+` converts nothing (ADR-0023).
  - Good: a typo in an assignment makes a local, not a global that other chunks see, and a function
    changes a global only where it declares one.
  - Good: a list holds `nil` and has an exact length, `in` finds a value in it, and a tuple can be a
    key by value, as in Python.
  - Good: Python programmers read `if items then` and `x or default` as they expect.
  - Bad: Lua code that assigns a global, or an outer function's local, from inside a function now
    makes a local there. A counter closure needs `nonlocal n`, and a function that sets a global
    needs `global name`. A porting rule of the linter reports an assignment that hides an outer
    local (M7.2).
  - Bad: a function-wide local is one variable for the whole call, so closures made in a loop share
    it, as in Python. An explicit `local` inside the loop gives each pass its own.
  - Bad: `x = x or default` now replaces a 0 or an empty string too, and `if count then` skips its
    block when `count` is 0. Ported Lua code that relied on 0 and `""` being true changes meaning
    without an error.
  - Bad: `f"..."` used to call a function named `f` with a string; that call is now written
    `f("...")` or `f "..."`.
  - Bad: `[[` still starts a long string, so a list whose first element is a list is written
    `[ [1, 2] ]`, with a space, which the formatter writes for it (M7.4).
  - Bad: the heap has three new kinds of object, each with a fixed charge (ADR-0021), and every
    truth test checks a value's type and, for a table, its count of entries.
  - Follow-up: M1.2 lexes f-strings; M1.5 parses list and tuple literals and f-string fields; M2.4
    counts each table's entries; M2.5 resolves names by the new scoping; M2.18 builds lists,
    tuples, and sets; M2.9 builds Python's truth and the collections' operators; M3.4 formats
    f-string fields; M3.15 adds the `list`, `tuple`, and `set` libraries; M7.2 adds the porting
    rules; M7.4 spaces nested lists.
- **Alternatives considered:**
  - Python's four types, with tables gone: closest to Python, but metatables, modules, and `_ENV`
    would need new designs, and so would every Lua idiom built on tables.
  - Lists and sets only: smaller, but a pair of values couldn't be a key by value.
  - Locals that reach the rest of their block, as Lua's `local` does: simpler to compile, but
    `if c then x = 1 else x = 2 end print(x)` would read a global `x`.
  - Lua's truth, which ADR-0024 kept: ported Lua code keeps its meaning, but Python programmers'
    expectations don't hold.
- **Revisit when:** a host reports porting bugs from the scoping or truth rules that no linter rule
  could catch, or needs tables to compare by value.

## ADR-0026 — Settle what f-strings, function-wide locals, collections, and truth leave open

- **Status:** Accepted
- **Date:** 2026-11-24
- **Context:** ADR-0025 records the owner's choices. Writing them into the language spec raised
  questions they don't answer. Python's f-strings use `:` for a format spec, where Ferrolua also
  uses `:` to call a method. Python's `global` statement covers one function, while Lua 5.5's
  `global` declarations also cover the functions nested in them. Python's lists, tuples, and sets
  live in a language without Lua's multiple results, metatables, or long strings. Python's truth
  rule has no case for a table, which is both a dictionary and an object. The agent proposed these
  answers on 2026-11-24, and the owner accepted them the same day.
- **Decision** (the owner accepted the agent's proposal on 2026-11-24):
  - **f-strings:**
    - The prefix is a lowercase `f` touching the opening `"`, `'`, or long bracket, as in
      `f[[...]]`; `f "x"`, with a space, is still a call.
    - `{{` and `}}` stand for `{` and `}`, and the text between fields keeps its string form's
      escapes.
    - At the top level of a field's expression, `!s` or `!r` before `:` or `}` is a conversion, and
      `:` starts the format spec unless a name and `(` follow it, which make a method call:
      `{p:length()}` calls `length`, and `{x:>8}` formats `x`. A spec may hold fields of its own,
      as in `{x:>{width}}`.
    - Without a spec, a field writes `tostring` of its value. `!r` writes a string as
      `string.format("%q")` does and any other value as `tostring` does, and `=` uses `!r` unless
      the field gives a conversion or a spec.
    - A table, userdata, or class instance with a `__format` metamethod is formatted by
      `__format(value, spec)`, which must return a string. A spec on any other value that isn't a
      number or a string raises an error.
    - The base function `format(value, spec)` formats one value as a field does, as Python's
      `format` does.
  - **Scoping:**
    - A `global` or `nonlocal` declaration covers the whole function it is in and every function
      nested in it, and must come before the function's first use of the name. A use before it is a
      compile error.
    - `nonlocal x` names the `x` of the nearest enclosing function that has one, the main chunk
      included; naming none is a compile error. `nonlocal` is a keyword only at the start of a
      statement and followed by a name.
    - `global *` makes every name that the function and its nested functions assign, and don't
      otherwise declare, a global, which restores Lua's rule for porting. `global <const> *` makes
      them read-only globals, so assigning one is a compile error.
    - Lua 5.5's implicit `global *` at the start of every chunk is gone, and so is its rule that a
      `global` declaration ends it. Reading an undeclared name is always a global read, and strict
      mode is the way to forbid it.
    - Reading a local before its function assigns it raises `local 'x' read before assignment`.
    - The rule that a self-recursive local function holds no reference to itself (ADR-0018) also
      covers a function-wide local assigned a function once, such as `function f() ... end` at a
      chunk's top level.
  - **Collections:**
    - A list or a tuple holds any value, `nil` included. Its index is an integer from `-#c` to
      `#c - 1`, and any other index raises an error, as Python's `IndexError` does. A list grows
      through its methods, such as `xs:append(v)`, never by assigning past its end.
    - `==` compares lists, tuples, and sets by value. `<`, `<=`, `>`, and `>=` compare lists and
      tuples element by element, and sets as subsets. `+` joins two lists or two tuples, `*` with an
      integer repeats one, and `|`, `&`, `-`, and `^` give the union, intersection, difference, and
      symmetric difference of two sets.
    - A tuple whose elements can all be keys hashes by value. A list or a set used as a table key or
      a set element raises an error, as Python's `unhashable type` does. Tables, functions,
      threads, and userdata still hash by handle (ADR-0020).
    - Each type has one metatable, as strings share theirs, whose `__index` is its library, so
      `xs:append(v)` calls `list.append`. `setmetatable` on a collection raises an error.
    - A generic `for` over a list, tuple, or set visits its values: `for v in xs do`. A set visits
      its members in an order the hash seed fixes, as a table's keys are.
    - The base functions `list(c)`, `tuple(c)`, and `set(c)` build a collection from another one or
      from a table's sequence, and an empty one with no argument.
    - The last expression in a list or tuple literal expands to all its results, as in a table
      constructor, so `[...]` collects the varargs.
    - Multiple results stay Lua's: `return a, b` returns two values, not a tuple, and `a, b = t`
      assigns the tuple to `a`; `t:unpack()` gives its elements as results.
    - `(x)` stays a parenthesized expression; `(x,)` is a tuple of one, and `()` the empty tuple.
    - `[[` always starts a long string.
    - A host's `Vec` converts to and from a list, and a `HashSet` to and from a set.
    - `tostring` of a collection writes its elements as `!r` does, as in `[1, "a"]`, `(1,)`, and
      `set([1, 2])`, so the text reads back as an equal value when its elements do. A collection
      that holds itself writes `...` where it repeats.
    - `string.join(sep, c)` joins the strings in a collection, so `(", "):join(xs)` works as
      Python's `", ".join(xs)` does.
  - **Truth:**
    - A table with no entries and no metatable is false, as an empty Python dictionary is. A table
      or userdata whose metatable has `__bool` is false when `__bool` returns `false`, and `__bool`
      must return a boolean. Every other table, userdata, and class instance is true.
    - `-0.0` is false and `nan` is true, as in Python.
    - The results of `__eq`, `__lt`, `__le`, and `__contains`, `table.sort`'s comparator, and
      `assert` use the new truth. Library functions that take `nil` or `false` to mean "no value",
      such as `string.gsub` with a replacement function and `load` with a reader, keep testing for
      exactly those two.
- **Consequences:**
  - Good: `{obj:name()}` works in an f-string, and so do the method calls Lua code already writes.
  - Good: `global *` at the top of a ported chunk keeps its assignments global, so a port can move
    to local scoping one function at a time.
  - Good: an object built as an empty table with a metatable stays true, as Python objects are.
  - Bad: an f-string format spec can't start with a name followed by `(`; none of Python's does.
  - Bad: `[ [1, 2] ]` needs its space, and `a, b = (1, 2)` doesn't unpack, unlike Python.
  - Bad: a truth test on a table reads its metatable as well as its count of entries.
  - Follow-up: M1.2 lexes the `f` prefix; M1.5 parses fields and collection literals; M2.5 builds
    the declarations and the error for a read before assignment; M2.9 builds truth with `__bool`;
    M2.18 builds the collections' operators, hashing, and metatables; M3.4 builds `__format` and
    `format`; M3.15 builds the `list`, `tuple`, and `set` libraries; M3.1 converts `Vec` and
    `HashSet`.
- **Alternatives considered:**
  - `:` always starting the format spec, as in Python: simpler, but `{p:length()}` would fail.
  - `global` covering one function, as Python's does: a ported chunk would need a declaration in
    every function that sets a global.
  - Lists that grow when code assigns `xs[#xs]`, as tables do: familiar from Lua, but Python raises
    an error there, and `append` says what it does.
  - Every empty table false, metatable or not: closer to Python's dictionaries, but an object built
    as an empty table with a metatable would be false.
  - `__bool` falling back to `__len`, as Python's `__bool__` falls back to `__len__`: one more
    lookup on every truth test of a table with a metatable.
  - A set literal `{1, 2}`, as Python writes it: `{1, 2}` already builds a table.
- **Revisit when:** a format spec that Python accepts fails in an f-string because of the
  method-call rule, or a host needs an empty table with a metatable to be false.

## ADR-0027 — Support Linux, Windows, macOS, Android, iOS, and the web

- **Status:** Accepted; the owner also accepted ADR-0028, which settles its details, on 2026-11-24
- **Date:** 2026-11-24
- **Context:**
  - [`03-platforms.md`](03-platforms.md) planned four targets in order: Linux on x86-64 first, then
    Linux on AArch64, macOS on AArch64, and Windows on x86-64, all three in the backlog until the
    first release. The platform that `.claude/rules/dependencies.md` defines included only Linux on
    x86-64. ADR-0021 already promises that bytes count alike on the six targets
    [Nadir](https://github.com/jabbott-iii/Nadir) builds for: x86-64 and AArch64 on Windows, macOS,
    and Linux.
  - On 2026-11-24 the owner asked for a plan to support Windows, macOS, Linux, Android, iOS, and
    the web.
  - An iOS app can't make memory executable without an entitlement Apple doesn't give ordinary
    apps, and a browser runs WebAssembly, not native machine code, so neither can run what
    Ferrolua's JIT writes.
  - Rust's `f64::sin`, `f64::exp`, and similar methods call each platform's math library, and those
    libraries can differ in a result's last bit. A script that uses `math.sin` can then print
    different digits on two targets, and a run recorded on one can replay differently on another.
  - Rust's test harness doesn't run in a browser, and the programs that run WebAssembly outside
    one, such as Node.js and Wasmtime, are third party (ADR-0002).
  - Asked the same day, the owner chose a JIT wherever the platform allows one, all six platforms
    by the first release, a browser test harness of Ferrolua's own, and math functions of
    Ferrolua's own that give the same bits everywhere.
- **Decision** (the owner chose the first of the options for each of four questions on
  2026-11-24):
  - **Six platforms.** Ferrolua runs on Linux, Windows, macOS, Android, iOS, and the web
    (WebAssembly in a browser), on the Rust targets [`03-platforms.md`](03-platforms.md) lists.
    The platform in `.claude/rules/dependencies.md` widens to each one's own tools: Microsoft's
    compiler tools and SDK for Windows, Apple's Xcode for macOS and iOS, Google's Android SDK and
    NDK, and the browsers that run WebAssembly.
  - **A JIT wherever allowed.** An x86-64 backend and an AArch64 backend give a JIT on Linux,
    Windows, macOS, and Android. iOS and the web run the interpreter only.
  - **All six by the first release.** The interpreter builds and passes its tests on every
    platform in CI from M3 on. A new milestone, M13, after M5, adds the AArch64 backend and the
    executable memory of Windows, macOS, and Android, and the first release (M12) ships all six.
  - **A browser test harness of Ferrolua's own.** An `xtask` command builds the tests to
    WebAssembly, serves them from a local server written with the standard library, and runs them
    in the headless browser on the CI runner. The browser is the web platform itself, so the
    harness adds no third-party code.
  - **The same math everywhere.** Ferrolua computes the functions that IEEE 754, the
    floating-point standard, doesn't define exactly, such as `math.sin`, `math.exp`, `math.log`,
    and `**` on floats, with code of its own, so every target gives the same bits.
  - [`03-platforms.md`](03-platforms.md) gives the targets, what each needs, and the invariants
    each passes. ADR-0028 (proposed) settles the details.
- **Consequences:**
  - Good: a host on any of the six embeds the same crate, and a recorded run replays on every
    target, which Nadir's replays across Linux, Windows, and macOS need.
  - Good: testing the interpreter on every platform from M3 on finds platform bugs while the code
    is young.
  - Good: hosts on iOS and the web, where no JIT can run, still get the whole language, its
    limits, and the same results.
  - Bad: a second JIT backend, for AArch64, and executable memory for three more operating
    systems, each with `unsafe` code under ADR-0013 (proposed).
  - Bad: CI builds and tests eleven Rust targets, including an Android emulator and an iOS
    simulator, which costs runner time on every pull request.
  - Bad: a math library of Ferrolua's own, whose last bit can differ from the C library's that
    PUC-Rio uses, so conformance scripts print math results with fewer digits than a float holds.
  - Bad: iOS and web hosts run only interpreted, far from the JIT's budgets.
  - Follow-up: M2.19 builds the math functions; M3.16 lets a host give a clock and a hash seed
    where the standard library has none; M3.17 builds the test runners for Android, iOS, and the
    web; M3.18 runs CI on every target; M13 builds the AArch64 backend and the executable memory of
    each operating system; M11's optimizing tier targets both backends; this change widens the
    platform in `.claude/rules/dependencies.md`.
- **Alternatives considered:**
  - A JIT on desktop only: less `unsafe` code, but Android hosts, many of them games, would lose
    it.
  - A WebAssembly JIT for the web: hot code would compile there too, at the cost of a third
    backend.
  - Desktop first, or Linux first, with the rest after the first release: less work before M12,
    but platform bugs would surface late, in code written without them in mind.
  - A WebAssembly runtime such as Node.js or Wasmtime: less code to write, but an exception to
    ADR-0002 (no third-party code).
  - The platform's math: no work, but outputs and replays would differ by target.
- **Revisit when:** a platform starts or stops letting apps make memory executable, or a host needs
  a platform beyond these six.

## ADR-0028 — Settle the targets, host services, math, and tests of the six platforms

- **Status:** Accepted
- **Date:** 2026-11-24
- **Context:** ADR-0027 records the owner's choices. Writing them into
  [`03-platforms.md`](03-platforms.md) raised questions they don't answer: which processors each
  platform gets, what the JIT does where the operating system refuses executable memory, what a
  browser lacks that the standard library expects, how processors differ in the bits of a NaN (a
  float that isn't a number), and how tests and coverage run on platforms with no test harness.
  The agent proposed these answers on 2026-11-24, and the owner accepted them the same day.
- **Decision** (the owner accepted the agent's proposal on 2026-11-24):
  - **Targets.** Linux on `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`; Windows on
    `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`; macOS on `aarch64-apple-darwin` and
    `x86_64-apple-darwin`; Android on `aarch64-linux-android` and `x86_64-linux-android`, which
    emulators and Chromebooks run; iOS on `aarch64-apple-ios`, with `aarch64-apple-ios-sim` for the
    simulator; and the web on `wasm32-unknown-unknown`. The library targets all eleven; the
    `ferrolua` command, the language server, and the debug adapter target Linux, Windows, and macOS.
  - **The JIT where it can't run.** On iOS and the web, `ferrolua-jit` compiles to nothing, so a
    host can turn the `jit` feature on for every target. There the default mode is `JitMode::Off`,
    and `Lua::new` refuses `On` and `Forced` with an error that names the target, as ADR-0022's
    refusal names the missing feature.
  - **The JIT where the system refuses.** Where the operating system refuses executable memory at
    run time, such as macOS for a hardened host without the `com.apple.security.cs.allow-jit`
    entitlement, a state in `JitMode::On` runs interpreted, and the host can ask the state whether
    its JIT is running. `JitMode::Forced` returns the error instead.
  - **Executable memory.** `mmap` and `mprotect` on Linux and Android; `VirtualAlloc`,
    `VirtualProtect`, `FlushInstructionCache`, and unwind tables registered with
    `RtlAddFunctionTable` on Windows; `mmap` with `MAP_JIT`, `pthread_jit_write_protect_np` on
    AArch64, and `sys_icache_invalidate` on macOS; and on Linux and Android for AArch64, the cache
    maintenance instructions, through inline assembly in `jit::memory`. ADR-0013 (proposed) lists
    them with its invariants.
  - **Services a browser lacks.** On `wasm32-unknown-unknown` the standard library has no clock
    and no source of randomness. `Options::clock` gives a state its wall clock and its processor
    clock; without one there, `os.time`, `os.clock`, and `os.date` with no time are absent.
    `Options::hash_seed` gives the seed; without one there, the state uses a fixed seed, and a host
    that runs untrusted scripts sets an instruction limit, which bounds hash flooding (ADR-0020).
    `io` and file-based `require` exist there only through a host's loader.
  - **Math.** A new crate, `ferrolua-math`, holds the functions, so constant folding, the VM's
    `**`, the JIT, and the `math` library share one implementation. Each result is within one unit
    in the last place of the exact value, and is computed with only addition, subtraction,
    multiplication, division, and square root, which IEEE 754 defines exactly, never with fused
    multiply-add or a platform's library. A stored file of inputs and result bits is checked on
    every target.
  - **NaN.** Every NaN prints as `nan`, as Python prints it, because processors give a new NaN
    different signs: x86-64 sets the sign bit in `math.huge - math.huge`, and AArch64 doesn't.
    `string.pack` still writes a NaN's bits as the processor made them, the one place they can
    differ.
  - **Tests.**
    - Rust's tests run natively on Linux, Windows, and macOS, on both processors. An x86-64 macOS
      build runs under Rosetta 2, Apple's translator, on an AArch64 runner if GitHub offers no
      Intel one.
    - On Android, `xtask` links the tests with the NDK's linker and runs them in the Android
      emulator through `adb`. On iOS, it runs them in the simulator through `xcrun simctl spawn`.
      The owner runs both on a device before each release.
    - On the web, the harness runs the conformance and extension scripts, the saved fuzz inputs,
      and the stored collection logs through `ferrolua`'s API. Rust's own unit tests, which its
      test harness can't run in a browser, run natively.
    - Coverage is measured on Linux, Windows, and macOS, and the floor holds on each, so code only
      one operating system compiles is covered there.
    - The hostile-input tests at the nesting limit also run on a thread with a 256 KB stack, so the
      parser and compiler fit the smaller stacks of mobile threads and browsers.
    - The file access policy's hostile-input tests include Windows paths: drive letters,
      backslashes, `..\`, and `\\?\` prefixes.
- **Consequences:**
  - Good: a host that builds for every target with one feature set and the default mode gets the
    JIT wherever one exists, with no code of its own per target.
  - Good: a hardened macOS host without the entitlement still runs, interpreted.
  - Good: a web host supplies the two services a browser lacks, and nothing else changes.
  - Bad: `JitMode::On` no longer guarantees compiled code; a host that needs it checks the state.
  - Bad: `string.pack` can still tell processors apart through a NaN's bits.
  - Bad: within one unit in the last place, not correctly rounded, so Ferrolua's math can differ
    from another correct library in the last bit.
  - Follow-up: M2.19 builds `ferrolua-math` and its stored results; M3.16 adds `Options::clock`;
    M3.4 prints NaN as `nan`; M3.17 builds the runners; M13.2 builds the executable memory of each
    operating system; M13.5 builds the fallback and the question the host asks.
- **Alternatives considered:**
  - Refusing `JitMode::On` where the system refuses executable memory: a host learns at once, but
    a hardened macOS host without the entitlement couldn't run at all.
  - Printing `-nan` and `nan` from the sign, as PUC-Rio does: matches PUC-Rio on each processor,
    but the same script would print differently on two targets.
  - Correctly rounded math, the closest float to the exact value every time: it would match every
    other correct library, but it takes longer to write and to run.
  - `wasm32-wasip1`, the WebAssembly System Interface, for the web: Rust's test harness runs there,
    but browsers don't provide it, and running it needs a third-party runtime.
- **Revisit when:** a host asks for `JitMode::On` to fail instead of falling back to the
  interpreter, or a target's math results differ from the stored bits.

## ADR-0029 — Show runtime errors with their source line, a caret, and a hint

- **Status:** Accepted
- **Date:** 2026-11-24
- **Context:**
  - The owner wants Ferrolua to stay easy to learn. On 2026-11-24 the owner asked the agent to
    draft this record as one of five improvements, and accepted it the same day.
  - A runtime error keeps PUC-Rio Lua 5.5.1's wording where Ferrolua shares the error, because
    scripts match on it ([`specs/vm.md`](specs/vm.md#errors)), and it names only a line:
    `game.lua:12: attempt to call a nil value (global 'pirnt')`.
  - Python 3.10 added "Did you mean" suggestions for misspelled names, and Python 3.11 prints the
    failing line with carets under the expression. Rust's compiler shows errors the same way, with
    hints. A learner who sees the expression and a likely fix spends less time on the error.
  - The tools' diagnostics already carry spans, labels, and notes
    ([`specs/diagnostics.md`](specs/diagnostics.md#the-diagnostic)), but runtime errors don't.
- **Decision** (the owner accepted the agent's proposal on 2026-11-24):
  - Each instruction's debug information records the byte span of the expression it came from, not
    only its line.
  - A runtime error's message stays exactly as it is, so `pcall` and scripts see no change. Beside
    it, `ferrolua::Error` carries the span and, where Ferrolua can work one out, a hint.
  - Ferrolua works out a hint only once an error reaches the host, never while the script runs:
    - for a name or field that isn't defined, the closest visible one within two edits, as in
      "did you mean `print`?";
    - for a local read before assignment, where an enclosing function has the same name,
      "add `nonlocal count`";
    - for `+` between a string and a number, "convert with `tostring(n)`, or use an f-string";
    - for indexing `nil` through `self` in a method called with `.`, "call it with `:`";
    - for a list index out of range, the list's length.
  - The `ferrolua` command and the debugger print a runtime error in the diagnostics' text format:
    the message, the file, line, and column, the source line with carets under the span, and the
    hint. `debug.traceback` keeps PUC-Rio's format.
- **Consequences:**
  - Good: a learner sees where an error happened and often how to fix it, and no script sees a
    different message.
  - Good: hints cost nothing until an error reaches the host.
  - Bad: debug information grows by a span for each instruction, in memory and in precompiled
    chunks.
  - Bad: each hint is code that must stay right, because a wrong hint misleads; each has a test.
  - Follow-up: M2.20 records the spans and builds the hints; M2.21 prints them in `ferrolua run`;
    M10.3 shows them in the debugger.
- **Alternatives considered:**
  - The hint inside the message string: every tool would show it, but scripts that match messages
    would break, and so would the comparison with PUC-Rio's wording.
  - Carets from line numbers alone: no new debug data, but a caret couldn't point inside a line.
- **Revisit when:** a user reports a hint that sent them the wrong way, or the spans make the
  conformance scripts' precompiled chunks more than a quarter larger.

## ADR-0030 — Give every state finite limits by default

- **Status:** Accepted
- **Date:** 2026-11-24
- **Context:**
  - Without a host limit, a state has no memory, instruction, or source-size limit
    ([`specs/sandbox.md`](specs/sandbox.md#limits)). The libraries already default to a safe set:
    `io`, `debug`, `package`, and most of `os` are off. ADR-0014 (proposed) makes the VM the
    sandbox, and ADR-0021 counts the instruction limit for each call from the host.
  - A host that forgets to set limits runs untrusted scripts with no bound: `while true do end`
    hangs it, and a table that keeps growing exhausts its memory.
  - On 2026-11-24 the owner asked the agent to draft this record as one of five improvements, and
    accepted it the same day.
- **Decision** (the owner accepted the agent's proposal on 2026-11-24):
  - `Options::default()` sets finite limits: 256 MB of memory, 1,000,000,000 instructions for each
    call from the host, and 16 MB for one chunk's source. Call depth and nesting keep their limits.
  - `Options::trusted()` grants every library and sets no limits, for a host that runs only its
    own scripts. `Limits::none()` turns the limits off and leaves the libraries as they are.
  - A limit error names the limit and the setting that raises it, such as
    `instruction limit of 1000000000 reached in one call (Limits::instructions)`.
  - `ferrolua run` uses `Options::trusted()`, because a person runs their own scripts there, and
    `ferrolua run --sandbox` uses the defaults.
  - M5.1's benchmarks check that no benchmark script reaches a default limit, and the numbers are
    revisited with those measurements.
- **Consequences:**
  - Good: a host that forgets the limits still bounds every script, so a hostile one ends with an
    error instead of a hang.
  - Good: a host reads the defaults in one place and raises the one it needs.
  - Bad: a host that embeds its own long-running scripts with the defaults gets a limit error, and
    must choose `Options::trusted()` or a higher limit.
  - Bad: the instruction counter runs by default; M5.1 measures what it costs.
  - Follow-up: M3.20 sets the defaults, `Options::trusted()`, and the messages.
- **Alternatives considered:**
  - No limits by default, as today: no surprise for trusted code, but every forgetful host is open.
  - `Options` with no default, so every host must choose: nothing is forgotten, but every example
    needs a page of settings.
  - A time limit: easy to understand, but wall-clock time differs between runs and machines, which
    the determinism of ADR-0018 forbids.
- **Revisit when:** a benchmark script, or a host's ordinary script, reaches a default limit.

## ADR-0031 — Specialize instructions in place in the interpreter

- **Status:** Accepted
- **Date:** 2026-11-24
- **Context:**
  - iOS and the web run only the interpreter (ADR-0027), and everywhere else so does every function
    the JIT hasn't compiled. A generic instruction such as `ADD` checks its operands' types each
    time it runs: integers, floats, strings, collections, or a metamethod.
  - CPython 3.11's specializing adaptive interpreter rewrites an instruction, once it has seen the
    same types, into a version for those types, and rewrites it back when they change. It was the
    largest part of a release that its notes say ran about 25% faster on average.
  - The baseline JIT needs to know which types each instruction saw
    ([`specs/jit.md`](specs/jit.md#baseline-tier)).
  - On 2026-11-24 the owner asked the agent to draft this record as one of five improvements, and
    accepted it the same day.
- **Decision** (the owner accepted the agent's proposal on 2026-11-24):
  - A prototype's code starts generic. After an instruction runs a fixed number of times with the
    same kinds of operands, the interpreter rewrites its opcode in place to a specialized form,
    such as an integer add, a float add, a read of a class instance's field slot, or a table read
    that remembers where a constant string key was found.
  - A specialized instruction checks its assumption first. When the check fails, it does the
    generic work, and after a fixed number of failures it rewrites itself back.
  - The specialized and generic forms give the same results, raise the same errors, and count the
    same work units (ADR-0020, ADR-0021), so limits stop a script at the same point either way.
    The counts are fixed numbers, so the same run rewrites the same instructions at the same
    moments every time.
  - Specialized opcodes exist only inside a state. `string.dump` writes the generic forms, and the
    verifier (ADR-0010) rejects a specialized opcode in a loaded chunk.
  - The baseline JIT reads the specialized opcodes as its record of the types each instruction
    saw.
  - Rewriting is safe Rust: the code is a vector of words in the prototype's arena slot.
- **Consequences:**
  - Good: every platform runs faster, iOS and the web most of all, since they have no JIT.
  - Good: the JIT gets its type feedback with no second mechanism.
  - Bad: many more opcodes, each specialized form tested against its generic one.
  - Bad: a prototype's code changes as it runs, so sharing compiled prototypes between states, which
    the backlog holds, needs a copy of the code in each state.
  - Follow-up: M5.10 builds it and measures it with M5.1's benchmarks; the differential tests also
    compare a run that specializes with one that doesn't.
- **Alternatives considered:**
  - A fast generic interpreter only: simpler, but every check runs every time.
  - Specializing at compile time from type annotations: free at run time, but most code has no
    annotations, and the VM never trusts them (ADR-0006).
  - A side table of caches without rewriting the opcode: the code stays fixed, but each
    instruction pays an extra lookup to find its cache.
- **Revisit when:** M5.10 measures the interpreted benchmarks less than 10% faster, or a bug traces
  to a specialized form that disagrees with its generic one.

## ADR-0032 — Grow strings and lists in place when only one place holds them

- **Status:** Accepted
- **Date:** 2026-11-24
- **Context:**
  - Strings can't change, and `+` joins two strings into a new one (ADR-0023), so `s += part` in a
    loop copies the whole string each time, and building a string from n parts takes time that
    grows with n². `xs = xs + [v]` copies a list the same way. The linter warns about it, and
    f-strings and `string.join` avoid it, but learners write the loop first.
  - CPython appends to a string in place when nothing else refers to it. Ferrolua counts
    references (ADR-0018), so it knows the same fact at no extra cost.
  - On 2026-11-24 the owner asked the agent to draft this record as one of five improvements, and
    accepted it the same day.
- **Decision** (the owner accepted the agent's proposal on 2026-11-24):
  - When the left operand of `+` is a string or a list that only the register receiving the result
    holds, as in `s = s + t` or `s += t` on a local, the VM appends to it in place instead of
    copying it. A string of 40 bytes or fewer, which the state interns, and any value with another
    reference, are copied as before.
  - The result equals what a copy would give, and nothing else can see the change, because nothing
    else refers to the value.
  - The buffer grows by Ferrolua's own rule and its bytes count by the fixed charges of ADR-0021.
    The JIT applies the same rule, so collections fall at the same points interpreted and
    compiled.
  - An append in place drops the string's cached hash.
  - `+=` keeps its meaning, `v = v + e`, so on a list that another variable also holds it makes a
    new list, unlike Python's `+=`, which extends the shared one.
- **Consequences:**
  - Good: the loop that learners write first runs in time that grows with n, not n².
  - Good: no new syntax and no new function.
  - Bad: the speed depends on whether a value is shared, so the same loop over a string that a
    table also holds stays slow; the lint rule stays for those cases.
  - Bad: the interpreter and the JIT must agree on the rule exactly, which the differential tests
    check.
  - Follow-up: M4.9 builds it in the VM, with a benchmark in `benches/lua/` that checks the time
    grows linearly; M5.5 and M13.3 apply it in compiled code.
- **Alternatives considered:**
  - Ropes, strings kept as trees of pieces: fast joins in every case, but every other string
    operation gets slower and more complex.
  - Python's in-place `+=` on lists: familiar to Python programmers, but a change to one variable
    would show through every other that holds the list, which `+=` on strings and numbers never
    does.
  - The lint rule alone: no VM change, but the slow loop stays the one learners write first.
- **Revisit when:** a benchmark shows the check for a single owner slowing `+` on shared strings, or
  a host reports a loop that stays slow because its string is shared.

## ADR-0033 — Start an interactive prompt from `ferrolua` with no arguments, in M3

- **Status:** Accepted
- **Date:** 2026-11-24
- **Context:**
  - The backlog holds "an interactive prompt for `ferrolua` with no arguments, as `lua` has",
    waiting for a user to ask. Python and Lua both start a prompt when run with no arguments, and
    learners try expressions there before they write a file.
  - With locals by default (ADR-0025), each line typed at a prompt is a chunk of its own, so every
    name a line assigns would be gone on the next line.
  - Arrow-key history and line editing need the terminal's raw mode, which takes `tcsetattr` or
    `SetConsoleMode`, and so `unsafe` code outside the JIT, which ADR-0013 (proposed) forbids.
  - On 2026-11-24 the owner asked the agent to draft this record as one of five improvements, and
    accepted it the same day.
- **Decision** (the owner accepted the agent's proposal on 2026-11-24):
  - `ferrolua` with no arguments starts a prompt when standard input is a terminal, which
    `std::io::IsTerminal` tells. Otherwise it runs standard input as a script, as `lua -` does.
  - A line that parses as an expression prints its value as an f-string's `!r` would write it, as
    Python's prompt does. A chunk that isn't finished continues on the next line, after a `...`
    prompt.
  - Each line compiles as if `global *` (ADR-0026) covered its top level, into a table for the
    session that falls back to the real globals, so `x = 1` on one line and `x` on the next work.
    Functions defined at the prompt keep local scoping inside.
  - Errors print as ADR-0029 shows them.
  - Line editing is the terminal's own until a way to set raw mode without `unsafe` code exists,
    which the backlog tracks.
  - The prompt uses `Options::trusted()` (ADR-0030), and `--sandbox` gives the defaults.
- **Consequences:**
  - Good: a learner tries code at once, and names persist between lines as in Python's prompt.
  - Bad: no arrow-key history at first; a user who wants it runs the prompt under a line-editing
    tool of their own.
  - Bad: a line at the prompt assigns into the session's table, while the same line in a file makes
    a local of its chunk.
  - Follow-up: M3.19 builds it; the backlog row for the prompt becomes one for line editing.
- **Alternatives considered:**
  - Keep the prompt in the backlog: no work now, but learners have nowhere to try code.
  - Each line's names lost, as Lua's prompt loses `local x`: simplest, but almost useless with
    locals by default.
  - Raw mode with `unsafe` code in `ferrolua-cli`: history from the start, but a second crate with
    `unsafe`, which ADR-0013 rules out.
- **Revisit when:** the standard library gains a way to set the terminal's raw mode, or a user
  reports being misled because a line behaves differently at the prompt than in a file.
