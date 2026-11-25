# Decision Log

This log records each significant or hard-to-reverse decision about Ferrolua as an architecture
decision record (ADR): what was decided, why, and what would change it. Records are numbered in the
order they were made, and an accepted record's decision is never edited: a new record supersedes
it. A record is `Accepted` only when the project's owner made or approved the decision; until then
it is `Proposed`, and no code may depend on it.

## ADR-0001 — Branch from Lua 5.5

- **Status:** Accepted
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

- **Status:** Accepted
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

- **Status:** Accepted
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

- **Status:** Accepted
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
- **Context:** ADR-0004 makes arity enforcement and undeclared-variable errors opt-in per chunk, by
  the host or by a directive in the file. Lua 5.5's `global` declarations already make an
  undeclared name an error inside their scope: a chunk that starts with `global none` may use only
  the names it declares.
- **Decision:**
  - A chunk turns strict mode on with a first line of `--!strict`. It is a comment, so PUC-Rio Lua
    5.5 ignores it, and the file still runs there.
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
  - Good: a strict file still runs on PUC-Rio Lua 5.5, unchecked.
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

- **Status:** Accepted
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

- **Status:** Accepted
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
