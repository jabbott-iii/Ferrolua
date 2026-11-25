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

## ADR-0012 — Build a method JIT for x86-64 with an assembler of Ferrolua's own

- **Status:** Proposed
- **Date:** 2026-11-24
- **Context:** A just-in-time (JIT) compiler turns hot bytecode into machine code while the program
  runs. There are two common designs:
  - A method JIT compiles whole functions.
  - A tracing JIT, like LuaJIT's, records the path a loop takes and compiles that path.
  Tracing JITs reach high speeds on loops, but they are harder to make correct and to debug. ADR-0002
  rules out assembler libraries, so Ferrolua writes its own x86-64 encoder.
- **Decision:** Ferrolua's JIT works in tiers:
  - The interpreter counts calls and loop iterations.
  - A hot function goes to a baseline tier, which translates each instruction into machine code,
    with guards on the types it has seen (M5).
  - A later optimizing tier builds a static single assignment (SSA) form of the function and
    optimizes it (M11).
  - When a guard fails, generated code exits to the interpreter at the same instruction, so the
    interpreter is always the reference.
  - The first backend is x86-64 with the System V calling convention, the one Linux uses.
  [`specs/jit.md`](specs/jit.md) gives the details.
- **Consequences:**
  - Good: each tier can be tested against the interpreter on the same scripts.
  - Good: a method JIT handles code without loops, such as callbacks, as well as loops.
  - Bad: the encoder is a large piece of work, which the overview sizes as L.
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
  - `jit::memory` maps, protects, and unmaps executable memory with Linux's `mmap`, `mprotect`, and
    `munmap`, declared in an `extern "C"` block against the C library the standard library already
    links.
  - `jit::entry` calls generated code.
  The code relies on these invariants:
  1. A page is writable or executable, never both (W^X).
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
- **Consequences:**
  - Good: one crate, two modules, and six invariants hold all of Ferrolua's `unsafe` code.
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

- **Status:** Accepted
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

- **Status:** Accepted
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
