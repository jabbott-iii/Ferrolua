# 00 — Overview

## Purpose

Lunaferra is a virtual machine (VM) with a just-in-time (JIT) compiler for a scripting language
derived from Lua 5.5, written in Rust, for Rust programs that embed one: games, editors, servers,
and plugin systems. Its language keeps Lua's statements, values, and libraries (ADR-0001: Lunaferra
branches from Lua 5.5), with Python's operators, indexes that start at 0, and ranges that stop
before their end (ADR-0023: a language of its own), and with Python's f-strings, locals by default,
lists, tuples, and sets, and rule for which values are false (ADR-0025). It adds what large Lua
programs lack: optional types, classes, compound assignment, and a strict mode that catches wrong
argument counts and undeclared variables. It also ships the tools that keep Lua code consistent,
fast, and secure, in one command: a type checker, a linter, a formatter, a static application
security testing (SAST) tool, a language server for editor auto complete, and a debugger.

One rule shapes the rest: **a script, however hostile, can't break memory safety or the host.**
Lunaferra uses no third-party code (ADR-0002), frees memory deterministically, by reference counting
and a loop collector that runs at points the program fixes (ADR-0018), keeps its heap in safe Rust (ADR-0011), and confines `unsafe` code
to the JIT's executable memory (ADR-0013). The VM is the sandbox: a script reaches only what its
host allows, within limits the VM enforces (ADR-0014).

## Goals

1. Lunaferra runs the language [`specs/language.md`](specs/language.md) defines. Where that
   language keeps Lua 5.5's behavior, a script prints what its Lua 5.5 twin prints on PUC-Rio
   Lua 5.5.1, apart from the intended differences the decision log records: ADR-0018 (memory is
   freed by reference counting and a loop collector), ADR-0010 (precompiled chunks use Lunaferra's
   own format), and ADR-0019 (objects print as IDs, never addresses). The conformance scripts in
   `tests/conformance/`, each paired with a reference twin, check this, interpreted and with the
   JIT forced on.
2. No script, precompiled chunk, or call from the host can read freed memory, panic, or crash the
   host process. Hostile-input tests and fuzz targets check every parser and loader of outside
   data, and no crate outside `lunaferra-jit` contains `unsafe`.
3. Memory is freed when its last reference goes, or, for reference loops, by the loop collector
   (ADR-0018). The same script with the same inputs and hash seed frees the same values in the
   same order on every run, interpreted or compiled, and a state never uses more memory than
   its host's limit.
4. A Rust program embeds Lunaferra by adding one crate, `lunaferra`, and `cargo tree` then lists
   only Lunaferra's own crates.
5. A host picks each state's libraries and limits, and a script can't exceed a memory, instruction,
   or call-depth limit (ADR-0014).
6. Scripts use Python's operators, 0-based indexes, half-open ranges, f-strings, locals by
   default, lists, tuples, sets, and truth, and can use optional types, classes, compound
   assignment, and strict mode, as [`specs/language.md`](specs/language.md) defines (ADR-0006
   through ADR-0008, and ADR-0023 through ADR-0026).
7. One binary, `lunaferra`, runs scripts and checks them: `run`, `check` (types), `lint`, `fmt`,
   and `sast`, with stable diagnostic codes and SARIF output for code scanning.
8. Editors get type-aware auto complete, live diagnostics, and a debugger through `lunaferra lsp`
   and `lunaferra dap` (ADR-0015).
9. Lunaferra meets the performance budgets below once they are agreed.
10. Lunaferra runs on Linux, Windows, macOS, Android, iOS, and the web, with the JIT on every one
    that allows executable memory, and the same script with the same inputs and hash seed gives
    the same output and the same collection log on every target (ADR-0027). CI tests each target
    listed in [`03-platforms.md`](03-platforms.md).

## Non-goals

- A collector that runs on a timer or on a thread of its own, or a tracing collector that
  replaces reference counting (ADR-0018).
- Third-party code, including test, lint, and format tools (ADR-0002).
- Running Lua scripts unchanged, whether written for Lua 5.5, an earlier version, or LuaJIT, and a
  mode that does: Lunaferra's operators, indexes, and ranges differ from Lua's (ADR-0001,
  ADR-0023).
- Checking types at run time (ADR-0006).
- Printing memory addresses, from `tostring`, `%p`, or anything else (ADR-0019).
- Loading chunks compiled by PUC-Rio's `luac` (ADR-0010).
- A JIT on iOS or the web, where an app can't run machine code it writes (ADR-0027).
- The Lua C API, and C modules built for PUC-Rio Lua (ADR-0017).

## Budgets

These targets are proposed and not yet agreed. The owner sets each one. The hardware isn't chosen
yet: M5.1 builds the benchmark runner and records the machine the budgets are measured on, and M5.9
makes each agreed budget a check that fails the build. PUC-Rio Lua 5.5.1 runs each benchmark's
Lua 5.5 twin in `benches/lua/reference/`. For reference, this development machine has
an Intel Core i7-11800H (x86-64, 16 threads) and 15 GB of memory, running Linux.

| Budget | Target (proposed) |
| :- | :- |
| Interpreter run time, geometric mean over `benches/lua/` | At most 1.25 times PUC-Rio Lua 5.5.1's time on the same machine |
| Baseline JIT run time, geometric mean over `benches/lua/` | At most 0.75 times PUC-Rio Lua 5.5.1's time |
| Optimizing JIT run time, geometric mean over `benches/lua/` | At most 0.33 times PUC-Rio Lua 5.5.1's time |
| Peak memory over `benches/lua/` | At most 1.25 times PUC-Rio Lua 5.5.1's peak resident memory |
| Longest loop collection over `benches/lua/` | At most 2 ms |
| Creating a state with the full standard library | At most 100 µs |
| `lunaferra check` and `lunaferra lint` on 100,000 lines of Lua | At most 2 s, from a cold start |
| Language server completion on a 5,000-line file | At most 50 ms for 95% of requests |

## Assumptions

Revisit these if any turns out to be wrong.

- PUC-Rio Lua 5.5.1 is the reference for what Lunaferra keeps from Lua 5.5, and Lua 5.5 bug-fix
  releases won't change output that conformance scripts depend on (ADR-0001).
- Users accept porting Lua code to Lunaferra's operators, indexes, and ranges (ADR-0023).
- Hosts are Rust programs. A C or C++ host would need the C API that ADR-0017 rules out.
- Each Lua state runs on one thread at a time. A host that needs parallel scripts runs one state
  per thread.
- A host that can't accept a collection pause at an allocation-driven point turns those runs off
  and collects at points it chooses, such as between frames (ADR-0018).
- Hosts on all six platforms embed Lunaferra as a Rust crate built for their target, and iOS and
  web hosts accept the interpreter's speed (ADR-0027).
- GitHub's runners, the Android emulator, and the iOS simulator stay available to CI for every
  target in [`03-platforms.md`](03-platforms.md).
- Contributors who record conformance output install PUC-Rio Lua 5.5.1 themselves. CI compares
  against the recorded output and doesn't need it.
- Editors that users choose have built-in clients for the Language Server Protocol (LSP) and the
  Debug Adapter Protocol (DAP), or users accept configuring one.

## Scope estimate

Sizes are for one experienced engineer working full time: S is up to one week, M is one to four
weeks, L is one to three months, and XL is more than three months.

| Area | Size | Why |
| :- | :- | :- |
| Workspace, coverage, conformance runner, and CI (M0) | M | Coverage through `llvm-tools` and a fuzzer, both of Lunaferra's own |
| Lexer, parser, and syntax trees (M1) | L | A lossless tree, error recovery, and two outputs from one parser (ADR-0009), with Python's precedence and conditional expressions |
| Bytecode and compiler (M2) | L | Register allocation, upvalues, Lua 5.5's `global` rules, and constant folding |
| VM core: heap, values, tables, interpreter, metamethods, coroutines (M2) | XL | Lua 5.5's behavior where the language keeps it, Python's results for operators, lists, tuples, sets, and truth, reference counting, a deterministic loop collector, and PUC-Rio's error messages |
| Standard library (M3) | L | `string.format` and number printing that match PUC-Rio byte for byte, patterns, `string.pack`, every position moved to count from 0, Python's format mini-language, and the `list`, `tuple`, and `set` libraries |
| Embedding API and sandbox (M3) | M | Rust types that tie values to their state, plus limits counted inside library calls |
| Language extensions (M4) | L | Classes with a fixed field layout, and grammar rules that fit around Python's operators |
| Baseline JIT for x86-64 (M5) | XL | An x86-64 encoder, executable memory, guards, and exits back to the interpreter |
| Type checker (M6) | XL | Gradual types, inference, narrowing, LuaCATS annotations, and types for the whole standard library |
| Linter and formatter (M7) | M | Shared analysis; a formatter that keeps comments and never changes meaning |
| SAST (M8) | L | Taint analysis across functions and files, and SARIF output |
| Language server (M9) | L | JSON, JSON-RPC, incremental analysis, and completion on code that doesn't parse yet |
| Debugger (M10) | M | VM hooks, the debug protocol, and debugging inside a running host |
| Optimizing JIT (M11) | XL | SSA form, register allocation, inline caches, and removing redundant counts, for both backends |
| Math functions with the same bits everywhere (M2.19) | M | Sine, exponent, logarithm, and power within one unit in the last place, from IEEE 754's exact operations only |
| Six platforms in CI (M3.16–M3.18) | M | Test runners for the Android emulator, the iOS simulator, and a browser harness of Lunaferra's own, plus eleven CI jobs |
| JIT on every platform that allows it (M13) | L | An AArch64 encoder and baseline tier, and executable memory, calling conventions, and unwind tables for Windows, macOS, and Android |
| First release (M12) | S | Release documents, API review by hand, and package checks |

## Top risks and mitigations

| Risk | Mitigation |
| :- | :- |
| A loop collection pauses a script longer than its host accepts (ADR-0018) | Self-recursion and classes make no loops; strings and prototypes are never visited; the linter flags loop patterns (M7.2); hosts can turn off collections by allocation count and collect between frames; a proposed budget caps the pause; the backlog holds an incremental collector |
| Something makes collections happen at different points between runs, between targets, or between the interpreter and the JIT | Only Lua heap bytes count toward the threshold, by fixed charges that no target or allocator changes (ADR-0021); nothing orders by address or hash; the conformance runner compares each script's collection log with the one stored for it, and a fuzz target compares the interpreter's with the JIT's (M5.8) |
| A bug in generated code breaks memory safety | `unsafe` stays in two modules under seven invariants (ADR-0013); every conformance script also runs with the JIT forced on; a fuzz target compares the interpreter with the JIT (M5.8) |
| Lunaferra's output differs from PUC-Rio's in details, such as float printing, `string.format`, or error messages | Conformance scripts compare with PUC-Rio's exact output for their Lua 5.5 twins; Lunaferra ports PUC-Rio's behavior, never its code; differences are fixed or recorded in a decision |
| Counting references makes the interpreter slower than PUC-Rio's | Benchmarks from M5.1 measure the cost; the optimizing JIT removes redundant counts (M11.2); ADR-0018 names the threshold that reopens the decision |
| A platform behaves differently: math results, a NaN's sign, stack sizes, or executable memory | `lunaferra-math` gives the same bits everywhere; every NaN prints as `nan`; nesting tests run on a 256 KB stack; the JIT falls back to the interpreter when a system refuses executable memory (ADR-0028); CI runs every target from M3.18 on |
| The plan is large for its team | Milestones deliver usable parts in order: a parser, then an interpreter, then embedding, then the extensions, the JIT, and the tools |
| Users won't port Lua code to Lunaferra's operators, 0-based indexes, scoping, and truth (ADR-0023, ADR-0025) | Lunaferra keeps Lua's statements, tables, and libraries, so porting changes expressions, index arithmetic, and some assignments; `global *` keeps a ported chunk's globals while it moves to local scoping; the linter's porting rules find `x ^ 2`, loops that start at 1, and assignments that hide an outer local (M7.2); the backlog holds a porting tool |
| A conformance script and its Lua 5.5 twin drift apart, so a pair passes while testing different things | Twins keep each statement on the same line and make the same calls in the same order; reviews read them side by side; the runner fails on a script without a twin |
| Without a coverage-guided fuzzer, deep bugs go unfound | Grammar-based generation of valid Lua, mutation of saved inputs, and differential testing between the interpreter and the JIT (see [`04-quality.md`](04-quality.md)) |
