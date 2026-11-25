# Roadmap

This file is the plan for building Ferrolua, from an empty workspace to the first release on
crates.io. Each milestone ends with something a user can see or use, and each task says what to
build, how to tell it worked, and the commands that check it. Why each choice was made is in
[`decisions.md`](decisions.md); how each part works is in [`specs/`](specs/).

## How to use this file

- Work from the top down. A task is unblocked when every task in its `Depends` line is ticked.
- Tick a task only after its `Accept` results hold and its `Verify` commands ran and passed, along
  with every check in "Definition of done" in [`../CLAUDE.md`](../CLAUDE.md). Those checks apply to
  every task, so `Verify` lists only the commands that belong to the task itself.
- A task with a `Decide first` line waits for the owner's answer before work starts.
- A milestone's exit criteria are checked by a person, who closes the milestone.
- Within each milestone, work happens on Linux on x86-64; other targets follow
  [`03-platforms.md`](03-platforms.md).
- A task that needs PUC-Rio Lua 5.5.1 to record output, on a machine without it, leaves its box
  unticked with a `⏳ needs PUC-Rio Lua 5.5.1` line naming the check that remains.

## M0 — A workspace where every check runs

**Goal:** A contributor can clone the repository and run every check, including coverage, the
fuzzer, and the conformance recorder, on an empty workspace, locally and in CI.

- [ ] **M0.1 — workspace: create the Cargo workspace**
  - Do:
    - Write the root `Cargo.toml` with every crate in the crate map of
      [`01-architecture.md`](01-architecture.md#crate-map) as a member, `[workspace.package]` with
      edition 2024, `rust-version = "1.100"`, and the MIT license, and `[workspace.lints]` as
      `.claude/rules/rust.md` describes.
    - Write `rust-toolchain.toml` pinning Rust 1.100.0 with Clippy, rustfmt, and `llvm-tools`, plus
      `clippy.toml` and `.cargo/config.toml` with the `xtask` alias.
    - Create each crate with a `lib.rs` or `main.rs` that holds only its crate-level doc comment,
      and commit `Cargo.lock`.
  - Accept:
    - `cargo build --workspace --locked` builds every crate with no warnings.
    - The tree command lists only Ferrolua's own crates.
  - Verify: `cargo build --workspace --locked`;
    `cargo tree --workspace --edges normal,build,dev --target all --prefix none` (expect only
    `ferrolua` crates and `xtask`)

- [ ] **M0.2 — testkit: seeded generator and property tests**
  - Depends: M0.1
  - Do:
    - Build `ferrolua-testkit` as [`04-quality.md`](04-quality.md#ferrolua-testkit) describes: a
      SplitMix64 seed expander, xoshiro256**, and a property runner that shrinks a failing case and
      prints its seed.
  - Accept:
    - The same seed gives the same sequence on every run, matching the reference outputs that the
      xoshiro256** authors publish.
    - A property that fails for values above 1,000 is reported with a shrunk case of 1,001.
  - Verify: `cargo test -p ferrolua-testkit --locked`

- [ ] **M0.3 — xtask: coverage command**
  - Depends: M0.1
  - Do:
    - Build `cargo xtask coverage` as [`04-quality.md`](04-quality.md#xtask) describes, finding
      `llvm-profdata` and `llvm-cov` under `rustc --print sysroot`.
  - Accept:
    - It prints line and region coverage for each crate, and fails below 100%.
    - Its own test, on a fixture crate with one uncovered function, sees it fail.
  - Verify: `cargo xtask coverage` (expect 100% of lines and regions); `cargo test -p xtask --locked`

- [ ] **M0.4 — fuzz: the fuzzer and regression replay**
  - Depends: M0.2
  - Do:
    - Build `ferrolua-fuzz` in `fuzz/` as [`04-quality.md`](04-quality.md#ferrolua-fuzz) describes:
      targets, mutation, a fixed seed, child processes with a time limit, shrinking, and saving to
      `fuzz/regressions/`.
    - Add `cargo xtask fuzz`, and a test that replays every saved input.
  - Accept:
    - A fixture target that panics on inputs starting with `FUZZ` is found from seed 0, shrunk to
      those 4 bytes, saved, and then replayed by `cargo test`.
    - A fixture target that loops forever is reported as a hang within its time limit.
  - Verify: `cargo xtask fuzz fixture-panic --seed 0 --runs 100000` (expect one saved input);
    `cargo test -p ferrolua-fuzz --locked`

- [ ] **M0.5 — xtask: conformance recorder**
  - ⏳ needs PUC-Rio Lua 5.5.1: recording the first `.out` file
  - Depends: M0.1
  - Do:
    - Build `cargo xtask record`, which runs PUC-Rio Lua 5.5.1 on scripts in `tests/conformance/` and
      writes their `.out` files, as [`04-quality.md`](04-quality.md#xtask) describes.
    - Add `tests/conformance/hello.lua`, which prints one line, and its recorded `.out` file.
  - Accept:
    - It refuses, with a clear message, an interpreter whose `lua -v` doesn't report Lua 5.5.1.
    - Recording a script twice gives the same `.out` file.
  - Verify: `cargo xtask record tests/conformance/hello.lua`; `cargo test -p xtask --locked`

- [ ] **M0.6 — ci: the CI workflow**
  - Depends: M0.3, M0.4
  - Do:
    - Write `.github/workflows/ci.yml`, running the automated checks of
      [`04-quality.md`](04-quality.md#automated-checks) and a fuzz run of 60 s per target, on
      `ubuntu-latest`, with `permissions: {}` at the top, each job's permissions set to the least
      it needs, and every action pinned to a full commit hash.
    - Update `CONTRIBUTING.md` and the pull request template to say CI runs the checks.
  - Accept:
    - A pull request shows every job, and a pull request with a formatting error fails.
  - Verify: open a pull request and observe its checks on GitHub; the owner then makes each job
    required for `main`

**Exit criteria (M0):** A fresh clone runs every check with the documented commands; CI runs them
on every pull request; the owner has made the jobs required.

---

## M1 — Parse Lua 5.5, with errors that point at the problem

**Goal:** `ferrolua parse` reads any Lua 5.5 file, reports syntax errors as PUC-Rio Lua 5.5.1 does,
and prints a lossless tree, which every later tool builds on.

- [ ] **M1.1 — syntax: source text, spans, and line maps**
  - Depends: M0.1
  - Do:
    - Build `Span`, `SourceId`, and `LineMap`, as [`specs/syntax.md`](specs/syntax.md#source-and-spans)
      describes.
  - Accept:
    - Line and column are right for every line ending PUC-Rio counts, and for sources that aren't
      valid UTF-8.
  - Verify: `cargo test -p ferrolua-syntax --locked`

- [ ] **M1.2 — syntax: lexer for Lua 5.5**
  - Depends: M1.1
  - Do:
    - Build the lexer of [`specs/syntax.md`](specs/syntax.md#lexer), with trivia on every token.
    - Add `tests/conformance/lexer.lua`, covering numbers, escapes, and long strings, with its
      recorded output.
  - Accept:
    - Every token form of §3.1 of the manual lexes, and every malformed form gives PUC-Rio's error,
      such as `malformed number near '0x'`.
  - Verify: `cargo test -p ferrolua-syntax --locked`

- [ ] **M1.3 — syntax: parser events and the lossless tree**
  - Depends: M1.2
  - Do:
    - Build the event stream and the lossless tree builder, with typed views for each node kind.
  - Accept:
    - Printing the tree gives back the source, byte for byte.
  - Verify: `cargo test -p ferrolua-syntax --locked`

- [ ] **M1.4 — syntax: statements**
  - Depends: M1.3
  - Do:
    - Parse every statement of §3.3 and §9 of the manual, including `global` declarations in every
      form, attributes, `goto` and labels, and named varargs.
  - Accept:
    - Each statement form parses to its node kind, and each nearby error has a test with its message.
  - Verify: `cargo test -p ferrolua-syntax --locked`

- [ ] **M1.5 — syntax: expressions and operator precedence**
  - Depends: M1.3
  - Do:
    - Parse expressions with the precedence and associativity of §3.4.8 of the manual, plus table
      constructors, function definitions, calls, and method calls.
  - Accept:
    - `2 ^ -3 ^ 2`, `-x ^ 2`, `a .. b .. c`, and `1 + 2 << 3` group as in PUC-Rio.
  - Verify: `cargo test -p ferrolua-syntax --locked`

- [ ] **M1.6 — syntax: error recovery and the diagnostic type**
  - Depends: M1.4, M1.5
  - Do:
    - Build the `Diagnostic` type of [`specs/diagnostics.md`](specs/diagnostics.md#the-diagnostic),
      and recovery as [`specs/syntax.md`](specs/syntax.md#parser-and-events) describes.
    - Add `tests/conformance/syntax_errors.lua`, which loads broken chunks and prints each message.
  - Accept:
    - A file with three separate errors gets three diagnostics, and the statements between them
      still appear in the tree.
    - `load`'s message for each broken chunk matches PUC-Rio's.
  - Verify: `cargo test -p ferrolua-syntax --locked`

- [ ] **M1.7 — syntax: compact tree for the compiler**
  - Depends: M1.6
  - Do:
    - Build the compact tree from the same events, as [`specs/syntax.md`](specs/syntax.md#compact-tree)
      describes.
  - Accept:
    - For generated valid programs, the compact tree matches the lossless tree's structure.
  - Verify: `cargo test -p ferrolua-syntax --locked`

- [ ] **M1.8 — syntax: size and nesting limits**
  - Depends: M1.6
  - Do:
    - Enforce the source size and nesting limits of [`specs/sandbox.md`](specs/sandbox.md#limits),
      with hostile-input tests in `crates/syntax/tests/hostile_syntax.rs`.
  - Accept:
    - Nesting at PUC-Rio Lua 5.5.1's limit parses, and one level more stops with PUC-Rio's message,
      both recorded in the syntax spec.
    - A 16 MB file parses in time proportional to its size.
  - Verify: `cargo test -p ferrolua-syntax --test hostile_syntax --locked`

- [ ] **M1.9 — fuzz: lexer and parser targets**
  - Depends: M0.4, M1.7
  - Do:
    - Add the Lua 5.5 program generator to `ferrolua-testkit`, and the `lexer` and `parser`
      targets, which check the round trip and that nothing panics.
  - Accept:
    - One million runs of each target from seed 0 find no crash, and every input they ever crashed on
      is saved.
  - Verify: `cargo xtask fuzz parser --seed 0 --runs 1000000` (expect no new saved input)

- [ ] **M1.10 — cli: `ferrolua parse`**
  - Depends: M1.6
  - Do:
    - Build the `ferrolua` binary's argument parser and the `parse` command of
      [`specs/cli.md`](specs/cli.md#commands).
  - Accept:
    - A valid file exits with 0, a broken one prints its diagnostics and exits with 1, and a missing
      file exits with 2.
  - Verify: `cargo test -p ferrolua-cli --locked`

**Exit criteria (M1):** `ferrolua parse` handles every script in `tests/conformance/`, reports
errors in PUC-Rio's words, and the fuzz targets run clean.

---

## M2 — Run Lua 5.5 scripts in the interpreter

**Goal:** `ferrolua run` runs scripts that use the core language and the base library, printing
what PUC-Rio Lua 5.5.1 prints, with memory freed by reference counting.

- [ ] **M2.1 — bytecode: instructions and prototypes**
  - Depends: M0.1
  - Do:
    - Define the instruction words and `Prototype` of [`specs/bytecode.md`](specs/bytecode.md), and
      record the full opcode list in that spec.
  - Accept:
    - Every opcode encodes and decodes to itself, and the spec lists every opcode with its operands.
  - Verify: `cargo test -p ferrolua-bytecode --locked`

- [ ] **M2.2 — vm: heap of counted slots and generational handles**
  - Depends: M0.1
  - Do:
    - Build the arenas, slots, handles, counting, and freeing work list of
      [`specs/memory.md`](specs/memory.md).
  - Accept:
    - A stale handle returns the internal error.
    - Dropping a 10,000,000-node list doesn't overflow the stack.
    - A dropped heap has freed every byte.
  - Verify: `cargo test -p ferrolua-vm --locked`

- [ ] **M2.3 — vm: values, strings, and interning**
  - Depends: M2.2
  - Do:
    - Build `Value`, strings, interning of short strings, and the per-state hash seed, random
      unless the caller gives one (ADR-0020), as [`specs/vm.md`](specs/vm.md#strings) describes.
  - Accept:
    - Two short strings with the same bytes share one handle, and two states get different seeds.
    - Two states given the same seed hash every string alike.
  - Verify: `cargo test -p ferrolua-vm --locked`

- [ ] **M2.4 — vm: tables**
  - Depends: M2.3
  - Decide first: whether a table operation's probe steps count against the instruction limit or
    are capped (ADR-0020).
  - Do:
    - Build tables with array and hash parts, key normalization, `#` borders, `next`, reference
      keys hashed by handle, and the probe bound, as [`specs/vm.md`](specs/vm.md#tables) describes.
  - Accept:
    - `#t` returns the border PUC-Rio Lua 5.5.1 returns for each table in
      `tests/conformance/length.lua`, once M2.14 can run it.
    - Two states given the same seed that build the same table, with strings, tables, and
      functions as keys, visit its keys in the same order.
    - With a cap, a set of keys crafted to collide under a known seed never takes more probe steps
      per operation than the cap; with counting, M3.2 checks the bound.
  - Verify: `cargo test -p ferrolua-vm --locked`

- [ ] **M2.5 — compiler: names, upvalues, globals, and self-recursion**
  - Depends: M1.7, M2.1
  - Do:
    - Build name resolution under Lua 5.5's `global` rules, upvalue capture, and the `SELF` rule of
      [`specs/compiler.md`](specs/compiler.md#self-recursion).
  - Accept:
    - A self-recursive local function has no upvalue for its own name; a reassigned one does.
    - Assigning to a `<const>` local, a `for` variable, or a `global <const>` is a compile error
      with PUC-Rio's message.
  - Verify: `cargo test -p ferrolua-compiler --locked`

- [ ] **M2.6 — compiler: expressions, registers, and constant folding**
  - Depends: M2.5
  - Do:
    - Compile every expression, allocate registers as [`specs/compiler.md`](specs/compiler.md#registers-and-expressions)
      describes, and fold constants.
  - Accept:
    - Folded and unfolded constant expressions give the same values, including at overflow and
      division by zero.
  - Verify: `cargo test -p ferrolua-compiler --locked`

- [ ] **M2.7 — compiler: statements and control flow**
  - Depends: M2.6
  - Do:
    - Compile every statement, including `goto`, both `for` forms, and `<close>` variables.
  - Accept:
    - Every prototype the compiler emits for the conformance scripts passes the verifier.
  - Verify: `cargo test -p ferrolua-compiler --locked`

- [ ] **M2.8 — vm: the interpreter loop, calls, and varargs**
  - Depends: M2.4, M2.7
  - Do:
    - Build the interpreter loop, frames, calls, returns, multiple results, varargs, named vararg
      tables, and proper tail calls.
  - Accept:
    - 10,000,000 tail calls in a loop use the stack of one.
  - Verify: `cargo test -p ferrolua-vm --locked`

- [ ] **M2.9 — vm: arithmetic, comparison, coercion, and number printing**
  - Depends: M2.8
  - Do:
    - Build integer and float arithmetic, bitwise operations, comparisons, concatenation, string
      to number coercion, and number to string conversion with PUC-Rio's exact rules, recording
      those rules in [`specs/vm.md`](specs/vm.md#the-interpreter).
    - Add `tests/conformance/arithmetic.lua` and `tests/conformance/tostring.lua`.
  - Accept:
    - Both scripts print their recorded output.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M2.10 — vm: metatables, metamethods, finalizers, and weak tables**
  - Depends: M2.9
  - Do:
    - Build every metamethod of [`specs/vm.md`](specs/vm.md#metamethods), finalizers at safe points,
      and weak tables, as [`specs/memory.md`](specs/memory.md) describes.
    - Record PUC-Rio's warning for an error in a finalizer in `specs/memory.md`.
    - Add `tests/conformance/metamethods.lua` and the scripts in `tests/extensions/gc/`.
  - Accept:
    - The conformance script prints its recorded output, and the extension scripts their written
      output.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M2.17 — vm: the deterministic loop collector**
  - Note: added on 2026-11-24, when the owner chose a deterministic loop collector on top of
    reference counting (ADR-0018). It is listed where it is to be done, after M2.10.
  - Depends: M2.10
  - Do:
    - Build candidate recording, trial deletion, the schedule by allocation count, the ephemeron
      rule, and the collection log, as [`specs/memory.md`](specs/memory.md#the-loop-collector)
      describes, plus the `collectgarbage` options in
      [`specs/memory.md`](specs/memory.md#collectgarbage).
    - Add `tests/conformance/gc.lua`, which builds loops, drops them, calls `collectgarbage()`, and
      prints what weak tables still hold, and the ordering scripts in `tests/extensions/gc/`.
  - Accept:
    - 100,000 dropped tables that each refer to themselves are freed by one `collectgarbage()`, and
      the heap returns to its size before.
    - No object a register, the registry, or the host can reach is ever freed, in the property test
      of the memory spec.
    - Two runs of every conformance script with a fixed hash seed give the same collection log.
    - `gc.lua` prints its recorded output.
  - Verify: `cargo test -p ferrolua-vm --locked`; `cargo test -p ferrolua --test conformance --locked`

- [ ] **M2.11 — vm: errors and protected calls**
  - Depends: M2.10
  - Do:
    - Build errors as values, `<close>` on unwinding, and the variable descriptions in runtime
      messages, as [`specs/vm.md`](specs/vm.md#errors) describes.
    - Add `tests/conformance/errors.lua`, printing each message caught by `pcall`.
  - Accept:
    - Every message matches PUC-Rio's, including `(global 'x')`, `(local 'x')`, `(field 'x')`,
      `(upvalue 'x')`, and `(method 'x')`.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M2.12 — vm: coroutines**
  - Depends: M2.11
  - Do:
    - Build threads, `resume`, `yield`, and `close` inside the VM, and the yield rules for native
      calls, as [`specs/vm.md`](specs/vm.md#coroutines) describes. The `coroutine` library comes in
      M3.8.
  - Accept:
    - VM tests resume and yield across `pcall`, and get PUC-Rio's error for a yield across
      `table.sort`.
  - Verify: `cargo test -p ferrolua-vm --locked`

- [ ] **M2.13 — stdlib: the base library**
  - Depends: M2.11
  - Do:
    - Build the base library of [`specs/stdlib.md`](specs/stdlib.md#libraries), with the argument
      checks shared by every library.
    - Assign object IDs as [`specs/memory.md`](specs/memory.md#object-ids) describes, and print
      them from `tostring` and `print` in the formats of
      [`specs/stdlib.md`](specs/stdlib.md#object-ids-in-output) (ADR-0019).
    - Add `tests/conformance/base.lua` and `tests/extensions/object_ids.lua`.
  - Accept:
    - Every base function, and each of its argument errors, prints its recorded output.
    - `object_ids.lua` prints its written output, and no `tostring` result contains an address.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M2.14 — ferrolua: a minimal embedding API and the conformance runner**
  - Depends: M2.13, M2.17
  - Do:
    - Build `Lua::new`, `Lua::load`, and `exec` and `eval` for `nil`, booleans, numbers, and strings, as
      [`specs/ferrolua.md`](specs/ferrolua.md) sketches.
    - Build the conformance runner in `crates/ferrolua/tests/conformance.rs`, as
      [`04-quality.md`](04-quality.md#conformance-scripts) describes.
  - Accept:
    - Every script in `tests/conformance/` prints its recorded output, and every script in
      `tests/extensions/` its written output.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M2.15 — cli: `ferrolua run`**
  - Depends: M2.14
  - Do:
    - Build `ferrolua run <script> [args...]`, with the `arg` table set as PUC-Rio's `lua` sets it.
  - Accept:
    - The conformance runner's binary pass prints every script's recorded output.
  - Verify: `cargo test -p ferrolua-cli --locked`

- [ ] **M2.16 — fuzz: compiler and interpreter targets**
  - Depends: M1.9, M2.14
  - Do:
    - Add the `compiler` and `interpreter` targets, which run generated programs under memory and
      instruction limits.
  - Accept:
    - One million runs of each from seed 0 find no crash.
  - Verify: `cargo xtask fuzz interpreter --seed 0 --runs 1000000` (expect no new saved input)

**Exit criteria (M2):** A person runs a few real Lua 5.5 scripts that use only the base library
with `ferrolua run`, and their output matches `lua`'s.

---

## M3 — Embed Ferrolua with the full standard library and a sandbox

**Goal:** A Rust program embeds Ferrolua, registers its own functions and types, runs untrusted
scripts within limits, and gets every Lua 5.5 library it grants.

- [ ] **M3.1 — ferrolua: the full embedding API**
  - Depends: M2.14
  - Decide first: whether `Lua` implements `Send`, as [`specs/ferrolua.md`](specs/ferrolua.md#open-questions)
    describes.
  - Do:
    - Build values tied to their state, `IntoLua` and `FromLua`, Rust functions, `UserData`, the
      registry, `Lua::collect`, the collector's options and `hash_seed`, and `Error`, then update
      the spec with the names the code settled.
  - Accept:
    - `Lua::collect` frees a loop the host dropped, and never a value the host still holds.
    - Using a value after its state is dropped fails to compile, in a `compile_fail` doc test.
    - Using a value with another state returns an error.
  - Verify: `cargo test -p ferrolua --locked`

- [ ] **M3.2 — vm: limits**
  - Depends: M3.1
  - Decide first: the owner accepts ADR-0014 (the VM is the sandbox), and decides whether pending
    finalizers and `<close>` variables run with a reserve after a limit stops a script.
  - Do:
    - Enforce every limit of [`specs/sandbox.md`](specs/sandbox.md#limits), including work units
      inside library calls, with hostile-input tests in `crates/ferrolua/tests/hostile_sandbox.rs`.
    - Run the loop collector once before the memory limit stops a script.
  - Accept:
    - A script whose only garbage is loops runs within a memory limit it would pass without the
      collector.
    - Each limit stops a script one unit past it, inside `pcall`, `xpcall`, metamethods, and
      coroutines alike.
    - If M2.4 chose counting, a set of keys crafted to collide under a known seed stops at the
      instruction limit (ADR-0020).
  - Verify: `cargo test -p ferrolua --test hostile_sandbox --locked`

- [ ] **M3.3 — stdlib: strings and patterns**
  - Depends: M2.13, M3.2
  - Do:
    - Build `string.byte` through `string.upper`, and the pattern functions of §6.4.1, counting
      matcher steps.
    - Add `tests/conformance/string.lua` and `tests/conformance/patterns.lua`.
  - Accept:
    - Both scripts print their recorded output, and a backtracking pattern stops at the
      instruction limit.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M3.4 — stdlib: `string.format` and number formatting**
  - Depends: M3.3
  - Do:
    - Build float formatting and every `string.format` conversion, as
      [`specs/stdlib.md`](specs/stdlib.md#number-formatting) describes.
    - Make `%p` print object IDs (ADR-0019), and record what it prints for values with no
      address.
    - Add `tests/conformance/string_format.lua`, with subnormal numbers, the largest and smallest
      floats, `inf`, and `nan`.
  - Accept:
    - The script prints its recorded output, and `%.17g` reads back every generated float.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M3.5 — stdlib: `string.pack`, `string.packsize`, and `string.unpack`**
  - Depends: M3.3
  - Do:
    - Build every format option of §6.4.2, and add `tests/conformance/pack.lua`.
  - Accept:
    - The script prints its recorded output, including each error for a malformed format.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M3.6 — stdlib: the `table` library**
  - Depends: M2.13
  - Do:
    - Build `table.create` and the rest of §6.6, with `table.sort` following PUC-Rio's algorithm,
      and add `tests/conformance/table.lua`.
  - Accept:
    - The script prints its recorded output, including the order of equal elements after `sort`.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M3.7 — stdlib: `math` and `utf8`**
  - Depends: M2.13
  - Do:
    - Build §6.7 with xoshiro256** seeded as PUC-Rio seeds it, and §6.5 with Lua 5.5's `lax`
      argument, and add `tests/conformance/math.lua` and `tests/conformance/utf8.lua`.
  - Accept:
    - `math.randomseed(42)` followed by ten `math.random(1, 100)` calls prints PUC-Rio's numbers.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M3.8 — stdlib: the `coroutine` library**
  - Depends: M2.12, M2.13
  - Do:
    - Build §6.2 on the VM's threads, and add `tests/conformance/coroutine.lua`.
  - Accept:
    - The script prints its recorded output, including `close` with pending `<close>` variables.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M3.9 — stdlib: `io` and `os`, under the file access policy**
  - Depends: M3.2
  - Do:
    - Build §6.8 and §6.9, the file access policies of [`specs/sandbox.md`](specs/sandbox.md#file-access),
      and `os.exit` returning an exit error.
    - Show file handles by their IDs in `tostring` (ADR-0019).
  - Accept:
    - Every escape path in the sandbox spec's tests is refused under a directory policy.
  - Verify: `cargo test -p ferrolua --test hostile_sandbox --locked`

- [ ] **M3.10 — stdlib: `package` and `require`**
  - Depends: M3.9
  - Do:
    - Build `require` with the preload and Lua searchers through the host's loader, and
      `package.loadlib` returning an error (ADR-0017), and add `tests/conformance/require/`.
  - Accept:
    - Modules load once, circular `require` behaves as in PUC-Rio Lua 5.5.1, and `package.loadlib`
      returns `nil` and a message.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M3.11 — stdlib: the `debug` library**
  - Depends: M2.12
  - Do:
    - Build §6.10 on Ferrolua's frames, and add `tests/conformance/debug.lua`.
    - Make `debug.upvalueid` return the upvalue's ID as an integer (ADR-0019).
  - Accept:
    - `debug.traceback` and `debug.getinfo` print PUC-Rio's output for the same script.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M3.12 — bytecode: precompiled chunks and the verifier**
  - Depends: M2.14
  - Do:
    - Build the chunk format, `string.dump`, binary `load`, and the verifier of
      [`specs/bytecode.md`](specs/bytecode.md#verifier), plus the `chunk` fuzz target.
  - Accept:
    - Each verifier rule rejects a chunk that breaks only that rule, and one million mutated
      chunks from seed 0 crash nothing.
  - Verify: `cargo test -p ferrolua-bytecode --locked`;
    `cargo xtask fuzz chunk --seed 0 --runs 1000000`

- [ ] **M3.13 — ferrolua: the library allowlist**
  - Depends: M3.2, M3.10, M3.11, M3.12
  - Do:
    - Build library selection, `Libraries::SAFE`, and `Libraries::ALL`, and the escape-attempt
      tests of [`specs/sandbox.md`](specs/sandbox.md#tests).
  - Accept:
    - A `SAFE` state has exactly the functions the sandbox spec lists.
  - Verify: `cargo test -p ferrolua --test hostile_sandbox --locked`

- [ ] **M3.14 — cli: options for `ferrolua run`**
  - Depends: M3.13
  - Do:
    - Add `-e`, `-l`, `--sandbox`, `LUA_INIT_5_5`, and `LUA_INIT`, as PUC-Rio's `lua` handles them.
  - Accept:
    - `ferrolua run --sandbox` refuses `io.open`, and `-e` and `-l` behave as in `lua`.
  - Verify: `cargo test -p ferrolua-cli --locked`

**Exit criteria (M3):** A person embeds Ferrolua in a small Rust program, registers a function and
a userdata type, and sees a hostile script stopped by each limit.

---

## M4 — Write Ferrolua: operators, types, classes, and strict mode

**Goal:** Scripts can use `++` and compound assignment, type annotations, classes, and strict
mode, as [`specs/language.md`](specs/language.md) defines, while every Lua 5.5 script still runs
unchanged.

- [ ] **M4.1 — syntax: compound assignment, `++`, and the superset test**
  - Depends: M1.9
  - Do:
    - Lex and parse the tokens and statements of
      [`specs/language.md`](specs/language.md#compound-assignment-and--adr-0005).
    - Add the superset property test of [`specs/language.md`](specs/language.md#tests).
  - Accept:
    - `x--=1` is `x` followed by a comment, and generated Lua 5.5 programs tokenize as under Lua 5.5
      alone.
  - Verify: `cargo test -p ferrolua-syntax --locked`

- [ ] **M4.2 — compiler: compound assignment and `++`**
  - Depends: M4.1, M2.14
  - Do:
    - Compile both forms, evaluating the target's parts once, and add
      `tests/extensions/compound_assignment.lua`.
  - Accept:
    - `t[f()] += g()` calls `f` once and `g` once, in that order, through metamethods too.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M4.3 — syntax: type annotations**
  - Depends: M4.1
  - Do:
    - Parse the annotation grammar of [`specs/language.md`](specs/language.md#type-annotations-adr-0006),
      have the compiler drop annotations and record JIT hints, and add the erasure property test.
  - Accept:
    - Generated typed programs compile to the same bytecode with and without their annotations,
      apart from hints.
  - Verify: `cargo test -p ferrolua-syntax -p ferrolua-compiler --locked`

- [ ] **M4.4 — syntax: class declarations**
  - Depends: M4.3
  - Decide first: the owner accepts ADR-0007 (classes with a fixed field layout), and answers its
    open questions in [`specs/language.md`](specs/language.md#classes-adr-0007-proposed).
  - Do:
    - Parse class declarations, and record the answers in the language spec.
  - Accept:
    - `class "Point"` is still a call, and `class Point ... end` is a class.
  - Verify: `cargo test -p ferrolua-syntax --locked`

- [ ] **M4.5 — vm: classes and instances**
  - Depends: M4.4
  - Do:
    - Compile and run classes, instances with a fixed layout, inheritance, `super`, and metamethod
      methods, and add `tests/extensions/classes/`.
  - Accept:
    - Creating and dropping 100,000 instances of a subclass returns the heap to its size before.
  - Verify: `cargo test -p ferrolua -p ferrolua-vm --locked`

- [ ] **M4.6 — compiler: strict mode and undeclared-variable errors**
  - Depends: M4.1
  - Decide first: the owner accepts ADR-0008 (strict mode with `--!strict` or a host option), and
    decides how a host lists its globals.
  - Do:
    - Build the directive, the host option, and undeclared-variable errors, as
      [`specs/language.md`](specs/language.md#undeclared-variable-errors) describes, recording
      PUC-Rio's message there.
  - Accept:
    - The example in the spec gives its error, and the file runs unchanged on PUC-Rio Lua 5.5.1.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M4.7 — vm: arity enforcement**
  - Depends: M4.6
  - Do:
    - Emit and run `ARITY` checks for strict functions, with optional parameters, as
      [`specs/language.md`](specs/language.md#arity-enforcement) describes, and add
      `tests/extensions/strict/`.
  - Accept:
    - Strict scripts that finish print the same output with strict mode off.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M4.8 — fuzz: the extensions in the generator**
  - Depends: M4.5, M4.7
  - Do:
    - Extend the program generator with every extension, and run the existing targets on it.
  - Accept:
    - One million runs of the `parser` and `interpreter` targets on extension programs crash
      nothing.
  - Verify: `cargo xtask fuzz interpreter --seed 1 --runs 1000000`

**Exit criteria (M4):** A person writes a small program using each extension, runs it, and runs
the same program's Lua 5.5 parts on PUC-Rio Lua 5.5.1.

---

## M5 — Run hot code as x86-64 machine code

**Goal:** Hot functions run as machine code from the baseline JIT, every script prints the same
output with the JIT on or forced, and the agreed budgets are checked.

- [ ] **M5.1 — xtask: benchmarks and the benchmark machine**
  - ⏳ needs PUC-Rio Lua 5.5.1: running the comparison
  - Depends: M3.13
  - Do:
    - Write the benchmark scripts in `benches/lua/` and `cargo xtask bench`, as
      [`04-quality.md`](04-quality.md#xtask) describes.
    - Record the benchmark machine and the first measurements in a note on this task and in
      [`00-overview.md`](00-overview.md#budgets).
  - Accept:
    - The runner prints the median time and peak memory of each script on Ferrolua and on
      PUC-Rio, the share of time spent updating counts and recording candidates, and the longest
      loop collection, for ADR-0018's thresholds.
  - Verify: `cargo xtask bench`

- [ ] **M5.2 — jit: the x86-64 encoder**
  - Depends: M0.1
  - Do:
    - Build `jit::asm`, as [`specs/jit.md`](specs/jit.md#the-encoder) describes.
  - Accept:
    - Each instruction form encodes to the bytes the Intel manual gives, with tests citing the page.
  - Verify: `cargo test -p ferrolua-jit --locked`

- [ ] **M5.3 — jit: executable memory**
  - Depends: M5.2
  - Decide first: the owner accepts ADR-0012 (a method JIT) and ADR-0013 (`unsafe` only in the
    JIT's memory and entry modules).
  - Do:
    - Build `jit::memory`, with the crate's own `[lints]` table, as ADR-0013 describes.
  - Accept:
    - A test that reads `/proc/self/maps` while code exists finds no mapping with both `w` and `x`.
  - Verify: `cargo test -p ferrolua-jit --locked`

- [ ] **M5.4 — jit: entry, exits, and the `Tier` trait**
  - Depends: M5.3, M2.14
  - Do:
    - Build `jit::entry`, the `Tier` trait in `ferrolua-vm`, and exits to the interpreter, with
      layout assertions for every `#[repr(C)]` type, as [`specs/vm.md`](specs/vm.md#the-compiled-tier)
      describes.
  - Accept:
    - A compiled function that does nothing but exit continues correctly in the interpreter.
  - Verify: `cargo test -p ferrolua-jit -p ferrolua-vm --locked`

- [ ] **M5.5 — jit: baseline tier for arithmetic, tables, and control flow**
  - Depends: M5.4
  - Do:
    - Emit templates with type guards for every instruction outside calls, as
      [`specs/jit.md`](specs/jit.md#baseline-tier) describes.
  - Accept:
    - Conformance scripts without calls print their output in `JitMode::Forced`.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M5.6 — jit: calls, returns, and on-stack replacement**
  - Depends: M5.5
  - Do:
    - Compile calls, returns, varargs, and tail calls, and enter hot loops through on-stack
      replacement.
  - Accept:
    - Every conformance script prints its output in `JitMode::Forced`.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M5.7 — jit: limits and debug hooks in compiled code**
  - Depends: M5.6
  - Do:
    - Count instructions and check memory in compiled code, and run functions with breakpoints or
      hooks interpreted.
    - Let the loop collector run only at safe points in compiled code, with every count exact.
  - Accept:
    - The limit tests of the sandbox spec pass in `JitMode::Forced`.
  - Verify: `cargo test -p ferrolua --test hostile_sandbox --locked`

- [ ] **M5.8 — ferrolua: JIT modes and differential fuzzing**
  - Depends: M5.7
  - Do:
    - Add `JitMode` to `Options`, `--jit` to `ferrolua run`, the forced pass to the conformance
      runner, and the `differential` fuzz target, each comparing output and collection logs.
  - Accept:
    - One million differential runs from seed 0 find no difference in output or collection log
      between the interpreter and the JIT.
  - Verify: `cargo xtask fuzz differential --seed 0 --runs 1000000`

- [ ] **M5.9 — xtask: budget checks**
  - Depends: M5.1, M5.8
  - Decide first: the owner agrees the budgets in [`00-overview.md`](00-overview.md#budgets).
  - Do:
    - Make `cargo xtask bench --check` fail when an agreed budget is exceeded, and add it to CI on
      the benchmark machine.
  - Accept:
    - A deliberately slowed build fails the check.
  - Verify: `cargo xtask bench --check`

**Exit criteria (M5):** A person runs the benchmarks with the JIT on and off, and sees the
interpreter and baseline budgets met.

---

## M6 — Type-check scripts before they run

**Goal:** `ferrolua check` reports type errors in annotated Lua and Ferrolua code, using inline
annotations, LuaCATS comments, and declarations for the standard library.

- [ ] **M6.1 — analysis: names, scopes, and symbols**
  - Depends: M4.3
  - Do:
    - Build name resolution and symbols on the lossless tree, as
      [`specs/analysis.md`](specs/analysis.md#names-and-scopes) describes.
  - Accept:
    - On generated programs, every name resolves as the compiler resolves it.
  - Verify: `cargo test -p ferrolua-analysis --locked`

- [ ] **M6.2 — analysis: control flow and data flow**
  - Depends: M6.1
  - Do:
    - Build control flow graphs, reaching definitions, and liveness.
  - Accept:
    - Unit tests give the expected graph for each statement kind, including `goto` and `break`.
  - Verify: `cargo test -p ferrolua-analysis --locked`

- [ ] **M6.3 — analysis: projects, modules, and `.ferrolua.lua`**
  - Depends: M6.2
  - Decide first: the owner accepts ADR-0016 (a data-only Lua configuration file).
  - Do:
    - Read `.ferrolua.lua` as [`specs/diagnostics.md`](specs/diagnostics.md#configuration-ferrolualua)
      describes, and link files through `require`.
  - Accept:
    - A configuration file with a function call is rejected with its message, and a project of
      three files linked by `require` resolves each module's value.
  - Verify: `cargo test -p ferrolua-analysis --locked`

- [ ] **M6.4 — types: representation and subtyping**
  - Depends: M6.1
  - Do:
    - Build the types of [`specs/types.md`](specs/types.md#types) and the subtype relation.
  - Accept:
    - Subtyping is reflexive and transitive on generated types, and recursive types terminate.
  - Verify: `cargo test -p ferrolua-types --locked`

- [ ] **M6.5 — types: inference, narrowing, and checking**
  - Depends: M6.2, M6.4
  - Do:
    - Build checking as [`specs/types.md`](specs/types.md#checking) describes, and register the
      first `T` codes in the diagnostics spec.
  - Accept:
    - Each `T` code has a file that triggers it and a near miss that doesn't.
  - Verify: `cargo test -p ferrolua-types --locked`

- [ ] **M6.6 — types: declaration files and the standard library**
  - Depends: M6.5
  - Do:
    - Settle the declaration syntax in [`specs/types.md`](specs/types.md#declarations), and write
      declarations for every Lua 5.5 library function.
  - Accept:
    - With `check_all` on, the conformance scripts give no `T` diagnostics beyond their marked
      lines.
  - Verify: `cargo test -p ferrolua-types --locked`

- [ ] **M6.7 — types: LuaCATS annotations**
  - Depends: M6.5
  - Do:
    - Read the LuaCATS tags of [`specs/types.md`](specs/types.md#luacats-annotations).
  - Accept:
    - A file annotated only with LuaCATS gets the same diagnostics as its inline-annotated twin.
  - Verify: `cargo test -p ferrolua-types --locked`

- [ ] **M6.8 — types: classes and strict arity**
  - Depends: M4.5, M4.7, M6.5
  - Do:
    - Check class fields, methods, and inheritance, and argument counts in strict files.
  - Accept:
    - Reading an undeclared field of an instance and a wrong argument count in a strict file each
      give their `T` diagnostic.
  - Verify: `cargo test -p ferrolua-types --locked`

- [ ] **M6.9 — json: values, parser, and writer**
  - Depends: M0.2
  - Do:
    - Build [`specs/json.md`](specs/json.md) without message framing, plus the `json` fuzz target.
  - Accept:
    - Every valid and invalid test document is accepted or rejected as RFC 8259 says.
  - Verify: `cargo test -p ferrolua-json --locked`

- [ ] **M6.10 — cli: `ferrolua check`**
  - Depends: M6.6, M6.9
  - Do:
    - Build `ferrolua check` with text and JSON Lines output, and the diagnostic registry test of
      [`specs/diagnostics.md`](specs/diagnostics.md#tests).
  - Accept:
    - The exit status follows [`specs/cli.md`](specs/cli.md#exit-status), and JSON output parses.
  - Verify: `cargo test -p ferrolua-cli --locked`

- [ ] **M6.11 — fuzz: analysis and type checker targets**
  - Depends: M6.6, M4.8
  - Do:
    - Add the `analysis` and `types` targets.
  - Accept:
    - One million runs of each from seed 0 crash nothing.
  - Verify: `cargo xtask fuzz types --seed 0 --runs 1000000`

**Exit criteria (M6):** A person annotates a real script, sees `ferrolua check` find a planted type
error, and sees no diagnostics on unannotated files.

---

## M7 — Lint and format scripts

**Goal:** `ferrolua lint` finds likely bugs, slow code, and reference loops, and `ferrolua fmt`
formats files without changing what they do.

- [ ] **M7.1 — lint: the rule engine, configuration, and suppression**
  - Depends: M6.3, M6.5
  - Do:
    - Build rule registration, severities from `.ferrolua.lua`, and suppression comments, as
      [`specs/diagnostics.md`](specs/diagnostics.md#suppression-comments) describes.
  - Accept:
    - Each comment form suppresses exactly the codes and lines it names.
  - Verify: `cargo test -p ferrolua-lint --locked`

- [ ] **M7.2 — lint: correctness and memory rules**
  - Depends: M7.1
  - Do:
    - Build the correctness and memory rules of [`specs/lint.md`](specs/lint.md#rules), registering
      each code.
  - Accept:
    - Each rule has a triggering file and a near miss, and `Class.__index = Class` is reported.
  - Verify: `cargo test -p ferrolua-lint --locked`

- [ ] **M7.3 — lint: performance and style rules**
  - Depends: M7.1
  - Do:
    - Build the performance and style rules, with fixes where they are certain.
  - Accept:
    - Every offered fix leaves the bytecode unchanged apart from the fixed construct.
  - Verify: `cargo test -p ferrolua-lint --locked`

- [ ] **M7.4 — fmt: layout and comments**
  - Depends: M1.7
  - Do:
    - Build the document layout and the printer of [`specs/fmt.md`](specs/fmt.md#layout).
  - Accept:
    - The meaning, idempotence, and comment property tests pass.
  - Verify: `cargo test -p ferrolua-fmt --locked`

- [ ] **M7.5 — fmt: options and the check mode**
  - Depends: M7.4, M6.3
  - Do:
    - Read the options of [`specs/fmt.md`](specs/fmt.md#options), and report `F` diagnostics.
  - Accept:
    - Every expected-output case passes with each option at its limits.
  - Verify: `cargo test -p ferrolua-fmt --locked`

- [ ] **M7.6 — cli: `ferrolua lint` and `ferrolua fmt`**
  - Depends: M7.2, M7.3, M7.5
  - Do:
    - Build both commands, with `--fix` and `--check`.
  - Accept:
    - `ferrolua fmt --check` exits with 1 on an unformatted file and 0 after `ferrolua fmt`.
  - Verify: `cargo test -p ferrolua-cli --locked`

- [ ] **M7.7 — ci: lint and format the repository's Lua scripts**
  - Depends: M7.6, M0.6
  - Do:
    - Format the scripts in `tests/` and `benches/`, run both commands on them in CI, and update the
      check commands in `.claude/rules/lua.md`.
  - Accept:
    - CI fails on a pull request that adds an unformatted script.
  - Verify: `ferrolua fmt --check tests benches`; `ferrolua lint tests benches`

**Exit criteria (M7):** A person runs both commands on a real project, and its formatted code
still passes its tests.

---

## M8 — Find security problems in Lua code

**Goal:** `ferrolua sast` follows untrusted data to dangerous functions across a project and
reports each flow, in text or as SARIF for code scanning.

- [ ] **M8.1 — sast: sources, sinks, and sanitizers**
  - Depends: M6.3, M6.5
  - Do:
    - Build the built-in lists of [`specs/sast.md`](specs/sast.md#sources-sinks-and-sanitizers) and
      the `sast` settings in `.ferrolua.lua`.
  - Accept:
    - A host-declared source and sink are honored like built-in ones.
  - Verify: `cargo test -p ferrolua-sast --locked`

- [ ] **M8.2 — sast: taint analysis across functions and files**
  - Depends: M8.1
  - Do:
    - Follow taint through assignments, calls, returns, table fields, and string operations.
  - Accept:
    - A flow through five functions in three files is reported with its whole path, and recursion
      terminates.
  - Verify: `cargo test -p ferrolua-sast --locked`

- [ ] **M8.3 — sast: the security rules**
  - Depends: M8.2
  - Do:
    - Build each rule of [`specs/sast.md`](specs/sast.md#rules), registering each code.
  - Accept:
    - Each rule reports its flow and stays quiet through a sanitizer.
  - Verify: `cargo test -p ferrolua-sast --locked`

- [ ] **M8.4 — sast: SARIF output**
  - Depends: M8.3, M6.9
  - Do:
    - Write SARIF 2.1.0 logs with rules, results, and code flows.
  - Accept:
    - The log has every field SARIF 2.1.0 requires, and GitHub accepts it in a test upload to a
      scratch repository.
  - Verify: `cargo test -p ferrolua-sast --locked`; upload a log with
    `github/codeql-action/upload-sarif` and observe the results under **Security**

- [ ] **M8.5 — fuzz: the SAST target**
  - Depends: M8.3, M6.11
  - Do:
    - Add the `sast` target.
  - Accept:
    - One million runs from seed 0 crash nothing.
  - Verify: `cargo xtask fuzz sast --seed 0 --runs 1000000`

- [ ] **M8.6 — cli: `ferrolua sast`**
  - Depends: M8.4
  - Do:
    - Build the command, with `--format sarif`.
  - Accept:
    - The exit status follows the CLI spec, and the SARIF output matches the library's.
  - Verify: `cargo test -p ferrolua-cli --locked`

**Exit criteria (M8):** A person runs `ferrolua sast` on a script with a planted command injection
and sees the flow reported, in the terminal and in GitHub's code scanning.

---

## M9 — Edit with auto complete and live diagnostics

**Goal:** An editor with an LSP client gets type-aware completion, diagnostics as you type, hover,
navigation, rename, and formatting from `ferrolua lsp`.

- [ ] **M9.1 — json: message framing and JSON-RPC**
  - Depends: M6.9
  - Do:
    - Build the framing and JSON-RPC layer of [`specs/json.md`](specs/json.md#message-framing).
  - Accept:
    - A header that lies about its length is rejected without reading past the limit.
  - Verify: `cargo test -p ferrolua-json --locked`

- [ ] **M9.2 — lsp: lifecycle, documents, and diagnostics**
  - Depends: M9.1, M7.2, M8.3
  - Do:
    - Build initialization, shutdown, document sync with incremental edits, and published
      diagnostics.
  - Accept:
    - An end-to-end test opens a file, edits it, and receives the right diagnostics after each edit.
  - Verify: `cargo test -p ferrolua-lsp --locked`

- [ ] **M9.3 — lsp: completion**
  - Depends: M9.2
  - Do:
    - Build completion as [`specs/lsp.md`](specs/lsp.md#features) describes, on unparsed code too.
  - Accept:
    - After `p.` where `p` is an instance, the items are exactly its fields and methods.
  - Verify: `cargo test -p ferrolua-lsp --locked`

- [ ] **M9.4 — lsp: hover, signature help, navigation, and symbols**
  - Depends: M9.2
  - Do:
    - Build hover, signature help, definition, references, and document symbols.
  - Accept:
    - Each request gives the expected result on the end-to-end test project.
  - Verify: `cargo test -p ferrolua-lsp --locked`

- [ ] **M9.5 — lsp: rename, formatting, and code actions**
  - Depends: M9.4, M7.5
  - Do:
    - Build rename across files, formatting, and code actions from diagnostic fixes.
  - Accept:
    - Rename changes exactly the references that find-references returns.
  - Verify: `cargo test -p ferrolua-lsp --locked`

- [ ] **M9.6 — lsp: the completion budget**
  - Depends: M9.3, M5.9
  - Do:
    - Make the completion benchmark a check that fails over the agreed budget.
  - Accept:
    - The check passes on the benchmark machine, and fails on a deliberately slowed build.
  - Verify: `cargo xtask bench --check`

- [ ] **M9.7 — cli: `ferrolua lsp` and editor setup**
  - Depends: M9.5
  - Do:
    - Build the command, and a guide in `docs/` to configuring Neovim, Helix, Emacs, and Zed.
  - Accept:
    - A person follows the guide in one editor and gets completion and diagnostics.
  - Verify: `cargo test -p ferrolua-cli --locked`; follow the guide in Neovim and observe completion
    after `p.`

**Exit criteria (M9):** A person edits a real project in an editor and gets completion and
diagnostics without noticeable delay.

---

## M10 — Debug scripts from an editor

**Goal:** An editor with a DAP client sets breakpoints, steps, and inspects variables in scripts
launched by `ferrolua dap` or running inside a host.

- [ ] **M10.1 — vm: the debugger interface**
  - Depends: M5.7
  - Do:
    - Build breakpoints, stepping, pausing, and frame inspection on the VM's hooks, as
      [`specs/vm.md`](specs/vm.md#debug-hooks) describes.
  - Accept:
    - A breakpoint stops before the line's first instruction every time it runs, with the JIT on.
  - Verify: `cargo test -p ferrolua-vm --locked`

- [ ] **M10.2 — dap: lifecycle, launch, and breakpoints**
  - Depends: M10.1, M9.1
  - Do:
    - Build the protocol's lifecycle, `launch`, and every breakpoint kind of
      [`specs/dap.md`](specs/dap.md#features).
  - Accept:
    - An end-to-end test launches a script, stops at a conditional breakpoint, and continues.
  - Verify: `cargo test -p ferrolua-dap --locked`

- [ ] **M10.3 — dap: stack, variables, and evaluation**
  - Depends: M10.2
  - Do:
    - Build stack traces, scopes, variables with expansion, `evaluate`, `setVariable`, and stopping
      on errors.
    - Show objects by the IDs `tostring` prints (ADR-0019).
  - Accept:
    - Variables shown for a frame are exactly the locals and upvalues in scope.
  - Verify: `cargo test -p ferrolua-dap --locked`

- [ ] **M10.4 — ferrolua: a debug server inside a host**
  - Depends: M10.3
  - Do:
    - Let a host serve the protocol over a loopback TCP port, off by default.
  - Accept:
    - An editor attaches to a running host, stops at a breakpoint, and the host resumes afterward.
  - Verify: `cargo test -p ferrolua --locked`

- [ ] **M10.5 — cli: `ferrolua dap` and editor setup**
  - Depends: M10.3
  - Do:
    - Build the command, and add debugging to the editor guide.
  - Accept:
    - A person follows the guide and debugs a script in one editor.
  - Verify: `cargo test -p ferrolua-cli --locked`; follow the guide in Neovim and step through a
    script

**Exit criteria (M10):** A person debugs a script inside a small host program from an editor.

---

## M11 — Faster machine code from types and profiles

**Goal:** The optimizing tier makes hot code faster, using what the baseline tier observed and the
hints from annotations, within the optimizing JIT budget.

- [ ] **M11.1 — jit: SSA form and type feedback**
  - Depends: M5.8
  - Do:
    - Build the SSA form of [`specs/jit.md`](specs/jit.md#optimizing-tier) from bytecode and the
      types the baseline tier recorded.
  - Accept:
    - Every conformance script prints its output with the optimizing tier forced.
  - Verify: `cargo test -p ferrolua --test conformance --locked`

- [ ] **M11.2 — jit: optimizations**
  - Depends: M11.1
  - Do:
    - Build constant and type propagation, guard removal, field caches by shape, inlining, and
      removal of count updates that cancel out.
  - Accept:
    - The differential target finds no difference, and the count-update share from M5.1 falls.
  - Verify: `cargo xtask fuzz differential --seed 2 --runs 1000000`; `cargo xtask bench`

- [ ] **M11.3 — jit: register allocation**
  - Depends: M11.1
  - Do:
    - Allocate machine registers by linear scan, spilling to the frame.
  - Accept:
    - Every exit rebuilds the interpreter's frame exactly, and every count is exact at each safe
      point, checked by the shadow run.
  - Verify: `cargo test -p ferrolua-jit --locked`

- [ ] **M11.4 — jit: hints from type annotations**
  - Depends: M11.2
  - Do:
    - Use the compiler's hints as first guesses, always behind a guard (ADR-0006).
  - Accept:
    - A wrong annotation gives the right result through an exit, never a wrong value.
  - Verify: `cargo test -p ferrolua-jit --locked`

- [ ] **M11.5 — xtask: the optimizing JIT budget**
  - Depends: M11.3, M11.4
  - Do:
    - Add the optimizing tier's budget to `cargo xtask bench --check`.
  - Accept:
    - The check passes on the benchmark machine.
  - Verify: `cargo xtask bench --check`

**Exit criteria (M11):** A person sees every budget in the overview met on the benchmark machine.

---

## M12 — First release on crates.io

**Goal:** Ferrolua's crates are on crates.io with documentation, and a user can follow a guide to
embed Ferrolua and use its language and tools.

- [ ] **M12.1 — docs: the release document**
  - Depends: M11.5
  - Do:
    - Write `docs/05-release.md`: build profiles, versioning, the publishing order of the crates,
      and the release checklist.
  - Accept:
    - The checklist names every step from tagging to publishing, in order.
  - Verify: read the rendered document

- [ ] **M12.2 — ferrolua: API review, READMEs, and docs.rs settings**
  - Depends: M12.1
  - Do:
    - Review every public item of every published crate by hand, give each crate a README, and set
      its docs.rs metadata.
  - Accept:
    - `cargo publish --dry-run` succeeds for every crate, in publishing order.
  - Verify: `cargo publish --dry-run -p <crate>` for each crate in order

- [ ] **M12.3 — docs: user guide**
  - Depends: M12.2
  - Do:
    - Write a guide to embedding Ferrolua and to its language extensions and tools, with examples
      that run as doc tests or conformance scripts.
  - Accept:
    - Every example in the guide runs.
  - Verify: `cargo test --workspace --locked`

- [ ] **M12.4 — ci: release checks**
  - Depends: M12.2
  - Do:
    - Add a workflow that runs the dry runs on each tag, pinned by commit hash, without a token.
  - Accept:
    - A test tag runs every dry run.
  - Verify: push a test tag and observe the workflow; the owner publishes with their own token

**Exit criteria (M12):** The owner publishes the crates, and a new user embeds Ferrolua by
following the guide.

---

## Backlog (each needs a trigger or a decision)

| Item | Trigger / reason |
| :- | :- |
| JIT and CI for Linux on AArch64, then macOS on AArch64, then Windows on x86-64 ([`03-platforms.md`](03-platforms.md)) | M12 ships, and the owner widens the platform for the next target |
| An interactive prompt for `ferrolua` with no arguments, as `lua` has | A user files a feature request for one |
| The Lua C API and C modules; today hosts must be Rust programs (ADR-0017) | A C or C++ host asks to embed Ferrolua |
| Incremental loop collection, which spreads one collection over several safe points to shorten pauses | A host measures a collection pause above the agreed budget that turning off automatic runs and scheduling collections can't avoid |
| Spreading the freeing of one large structure over several safe points | A benchmark measures a pause over 1 ms from freeing one value |
| Deferred counting of register references, which changes when `__gc` runs and needs a new record | M5.1 shows count updates above ADR-0018's threshold after M11.2 |
| Running lua.org's official 5.5.1 test suite; it isn't part of the platform today | The owner adds it to the platform in a decision record |
| Coverage-guided fuzzing; stable Rust can't read coverage counters while running | A stable rustc option exposes coverage counters in process |
| Branch coverage instead of regions | rustc stabilizes branch coverage |
| A mutation testing tool of Ferrolua's own; none runs today | A bug ships past a test that ran its code without asserting on the result |
| A Visual Studio Code extension (ADR-0015) | The owner decides to publish one, with its client either written here or approved as an exception |
| Sharing compiled prototypes between states | A host measures the cost of loading the same scripts into many states |
| Re-recording conformance output for a new Lua 5.5 release | lua.org releases Lua 5.5.2 |
