# 01 — Architecture

Ferrolua is a Cargo workspace of small crates in two stacks that share a parser. The runtime stack
turns source into bytecode, runs it, and compiles hot code to machine code. The analysis stack
turns source into a semantic model that the type checker, linter, and SAST tool read. Front ends,
the `ferrolua` command and the editor servers, sit on top. None of the crates exists yet: M0.1
creates the workspace, and the crate map below names the task that builds each crate.

## Principles

- **Memory safety comes first.** No crate uses `unsafe` except `ferrolua-jit`, and there only in
  the two modules that ADR-0013 (only the JIT's memory and entry modules may use `unsafe`) names.
  A host links `ferrolua-jit` only by turning on the `ferrolua` crate's `jit` feature, which is off
  by default (ADR-0022: the JIT is an optional feature of `ferrolua`, off by default). The heap is
  safe Rust (ADR-0011: values live in arenas and are reached through checked handles).
- **The interpreter is the reference.** Every tier of the JIT must produce what the interpreter
  produces. When generated code meets a case it didn't plan for, it exits to the interpreter at the
  same instruction (ADR-0012).
- **Errors are values, never panics.** A Lua error travels through Rust as an `Err` and unwinds Lua
  frames one by one. A panic is always a bug, and no input may cause one.
- **Every limit is enforced inside the VM.** Memory, instruction, and depth limits hold inside
  library calls too, so a host never depends on a script's cooperation (ADR-0014).
- **One parser feeds everything.** The compiler and the tools read the same parse, so they agree
  on every program (ADR-0009).
- **No third-party code** (ADR-0002). Ferrolua writes what it needs, in a crate of its own, with
  tests.

## Crate map

Each crate lives in `crates/<name>/`, except the fuzzer in `fuzz/` and the build tasks in `xtask/`.
Commits use the name without the `ferrolua-` prefix as their scope, as `.claude/rules/commits.md`
describes; the `ferrolua` crate's scope is `ferrolua`.

| Crate | Job | Depends on | crates.io | Spec | Lands in |
| :- | :- | :- | :- | :- | :- |
| `ferrolua-syntax` | Lexer, parser, syntax trees, spans, and the shared diagnostic type | — | Yes | [syntax](specs/syntax.md) | M1 |
| `ferrolua-bytecode` | Instruction set, function prototypes, chunk format, and verifier | — | Yes | [bytecode](specs/bytecode.md) | M2.1 |
| `ferrolua-math` | Floating-point functions with the same bits on every target (ADR-0027) | — | Yes | [math](specs/math.md) | M2.19 |
| `ferrolua-compiler` | Compact tree to bytecode: scopes, registers, strict mode | syntax, bytecode, math | Yes | [compiler](specs/compiler.md) | M2.5 |
| `ferrolua-vm` | Heap and loop collector, values, tables, interpreter, metamethods, coroutines, limits, debug hooks | bytecode, compiler, math | Yes | [vm](specs/vm.md) | M2.2 |
| `ferrolua-jit` | x86-64 and AArch64 encoders, executable memory, baseline and optimizing tiers | vm, bytecode, math | Yes | [jit](specs/jit.md) | M5 |
| `ferrolua-stdlib` | The standard libraries, Lua 5.5's with positions from 0 | vm, math | Yes | [stdlib](specs/stdlib.md) | M2.13 |
| `ferrolua` | The embedding API that hosts depend on | vm, compiler, stdlib; jit with its `jit` feature | Yes | [ferrolua](specs/ferrolua.md) | M2.14 |
| `ferrolua-analysis` | Semantic model: scopes, symbols, control flow, data flow, modules | syntax | Yes | [analysis](specs/analysis.md) | M6.1 |
| `ferrolua-types` | Gradual type checker | analysis, syntax | Yes | [types](specs/types.md) | M6.4 |
| `ferrolua-lint` | Lint rules | analysis, types | Yes | [lint](specs/lint.md) | M7.1 |
| `ferrolua-fmt` | Formatter | syntax | Yes | [fmt](specs/fmt.md) | M7.4 |
| `ferrolua-sast` | Taint analysis and security rules, with SARIF output | analysis, types, json | Yes | [sast](specs/sast.md) | M8.1 |
| `ferrolua-json` | JSON values, parser, writer, and protocol message framing | — | Yes | [json](specs/json.md) | M6.9 |
| `ferrolua-lsp` | Language server | json, analysis, types, lint, fmt, sast | Yes | [lsp](specs/lsp.md) | M9 |
| `ferrolua-dap` | Debug adapter | json, ferrolua | Yes | [dap](specs/dap.md) | M10 |
| `ferrolua-cli` | The `ferrolua` command | all of the above | Yes | [cli](specs/cli.md) | M1.10 |
| `ferrolua-testkit` | Seeded random generator, property tests, Lua source generator | — | No: a dev-dependency only | [04-quality](04-quality.md#the-projects-own-test-tools) | M0.2 |
| `ferrolua-fuzz` | The fuzzer and its targets, in `fuzz/` | testkit, the crates it targets | No | [04-quality](04-quality.md#the-projects-own-test-tools) | M0.4 |
| `ferrolua-webtest` | The WebAssembly test module the browser harness runs, in `xtask/webtest/` | ferrolua | No | [04-quality](04-quality.md#the-projects-own-test-tools) | M3.17 |
| `xtask` | `cargo xtask coverage`, `record`, `test`, and `bench` | — | No | [04-quality](04-quality.md#the-projects-own-test-tools) | M0.3 |

Four specs cover contracts that span crates: [`language.md`](specs/language.md) (Ferrolua's
language), [`memory.md`](specs/memory.md) (reference counting and the heap),
[`sandbox.md`](specs/sandbox.md) (libraries and limits), and
[`diagnostics.md`](specs/diagnostics.md) (codes, output formats, and configuration).

## Layers

A crate may depend only on crates in lower layers, or on lower crates in its own stack:

| Layer | Runtime stack | Analysis stack |
| :- | :- | :- |
| 3. Front ends | `ferrolua-cli`, `ferrolua-dap` | `ferrolua-cli`, `ferrolua-lsp` |
| 2. Services | `ferrolua` (embedding API) | `ferrolua-lint`, `ferrolua-sast`, `ferrolua-fmt` |
| 1. Engines | `ferrolua-compiler`, `ferrolua-vm`, `ferrolua-stdlib`, `ferrolua-jit` | `ferrolua-analysis`, `ferrolua-types` |
| 0. Foundations | `ferrolua-syntax`, `ferrolua-bytecode`, `ferrolua-json` | `ferrolua-syntax`, `ferrolua-json` |

- The runtime stack never depends on the analysis stack, so a host that embeds Ferrolua doesn't
  build the type checker. The compiler reads type annotations from the syntax tree and stores them
  as hints in the bytecode; the JIT reads the hints, never the type checker (ADR-0006).
- `ferrolua-vm` doesn't depend on `ferrolua-jit`. The VM defines a trait for a compiled tier, and
  the `ferrolua` crate plugs the JIT into it when its `jit` feature is on, so the VM, and a host
  that leaves the feature off, build and run without machine code.
- Nothing depends on a front end.

The diagram shows what depends on what. Every part is drawn dashed because none is built yet; a
part is redrawn solid in the change that builds it.

```mermaid
flowchart TB
  classDef planned stroke-dasharray: 5 5

  cli["ferrolua-cli<br/>(M1.10)"]:::planned
  lsp["ferrolua-lsp<br/>(M9)"]:::planned
  dap["ferrolua-dap<br/>(M10)"]:::planned
  api["ferrolua<br/>(M2.14)"]:::planned
  lint["ferrolua-lint<br/>(M7.1)"]:::planned
  sast["ferrolua-sast<br/>(M8.1)"]:::planned
  fmt["ferrolua-fmt<br/>(M7.4)"]:::planned
  types["ferrolua-types<br/>(M6.4)"]:::planned
  analysis["ferrolua-analysis<br/>(M6.1)"]:::planned
  jit["ferrolua-jit<br/>(M5)"]:::planned
  stdlib["ferrolua-stdlib<br/>(M2.13)"]:::planned
  vm["ferrolua-vm<br/>(M2.2)"]:::planned
  compiler["ferrolua-compiler<br/>(M2.5)"]:::planned
  bytecode["ferrolua-bytecode<br/>(M2.1)"]:::planned
  syntax["ferrolua-syntax<br/>(M1)"]:::planned
  json["ferrolua-json<br/>(M6.9)"]:::planned
  math["ferrolua-math<br/>(M2.19)"]:::planned

  cli -.-> api
  cli -.-> lsp
  cli -.-> dap
  cli -.-> lint
  cli -.-> sast
  cli -.-> fmt
  cli -.-> types
  lsp -.-> types
  lsp -.-> lint
  lsp -.-> sast
  lsp -.-> fmt
  lsp -.-> json
  dap -.-> api
  dap -.-> json
  api -. jit feature .-> jit
  api -.-> stdlib
  api -.-> compiler
  jit -.-> vm
  stdlib -.-> vm
  vm -.-> compiler
  vm -.-> bytecode
  compiler -.-> syntax
  compiler -.-> bytecode
  compiler -.-> math
  vm -.-> math
  stdlib -.-> math
  jit -.-> math
  lint -.-> types
  sast -.-> types
  sast -.-> json
  types -.-> analysis
  analysis -.-> syntax
  fmt -.-> syntax
```

## Source layout

```text
Cargo.toml            the workspace: members, [workspace.package], [workspace.lints]
Cargo.lock
rust-toolchain.toml   Rust 1.100.0 with Clippy, rustfmt, and llvm-tools
clippy.toml
.cargo/config.toml    the `cargo xtask` alias
crates/<name>/        one folder per crate in the crate map
fuzz/                 ferrolua-fuzz, with corpus/ and regressions/
xtask/                coverage, conformance recording, test runners, and benchmarks
  webtest/            ferrolua-webtest, and the page the browser harness opens
tests/conformance/    Ferrolua scripts for behavior kept from Lua 5.5
  reference/          their Lua 5.5 twins, each with the .out file PUC-Rio Lua 5.5.1 printed
tests/extensions/     scripts for Ferrolua's own behavior, with .out files written from the specs
benches/lua/          Ferrolua benchmark scripts, with their Lua 5.5 twins in reference/
docs/                 these documents
```

## Running a script

1. The host creates a state with `ferrolua::Lua::new`, choosing its libraries, limits, and, with
   the `jit` feature, its JIT mode ([`specs/ferrolua.md`](specs/ferrolua.md)).
2. `Lua::load` passes the source to `ferrolua-syntax`, whose parser builds the compact tree. A
   syntax error returns a message in PUC-Rio's format, such as
   `input:3: unexpected symbol near 'end'`.
3. `ferrolua-compiler` resolves names under the scoping rules of
   [`specs/language.md`](specs/language.md#scoping-adr-0025), applies strict mode where it is on
   (ADR-0008), allocates registers, and emits a function prototype in `ferrolua-bytecode`'s format.
4. `ferrolua-vm` runs the prototype in its interpreter. Each value is reference counted
   ([`specs/memory.md`](specs/memory.md)); when a count reaches zero, the value is freed, and a
   `__gc` finalizer, if it has one, runs at the next safe point. Values that only reference
   loops keep alive are freed by the loop collector, which runs at a safe point when the bytes
   allocated pass the host's threshold, or when the script or host asks (ADR-0018: a
   deterministic loop collector on top of reference counting).
5. The interpreter counts calls and loop iterations. With the `jit` feature on, when a function
   gets hot, the JIT compiles it, and later calls run the machine code. A failed guard exits to the
   interpreter at the same instruction ([`specs/jit.md`](specs/jit.md)).
6. A Lua error returns `Err` through each frame until a `pcall` catches it or it reaches the host.
   A limit error passes through `pcall` to the host ([`specs/sandbox.md`](specs/sandbox.md)).
7. Dropping the state runs the pending finalizers, then frees every value left.

## Checking a script

1. `ferrolua check`, `lint`, or `sast`, or the language server, reads `.ferrolua.lua` (ADR-0016:
   the tools read a data-only Lua file) and the files it names.
2. `ferrolua-syntax` builds a lossless tree for each file, with errors marked in place, so a file
   that doesn't parse still gets checked.
3. `ferrolua-analysis` resolves names, builds each function's control flow graph, and links the
   files through `require` calls with literal names.
4. `ferrolua-types` infers and checks types; `ferrolua-lint` and `ferrolua-sast` run their rules on
   the typed model.
5. Every tool reports the same diagnostic type, which the command line prints as text, JSON, or
   SARIF, and the language server sends to the editor
   ([`specs/diagnostics.md`](specs/diagnostics.md)).

## Threads

No crate starts a thread or an async runtime. A Lua state is used from one thread at a time, and
its coroutines run on the thread that resumes them. A host that needs scripts in parallel creates
one state per thread; states share nothing. Whether a state can move between threads, that is,
whether `Lua` implements `Send`, is settled in M3.1. The debug server in a host (M10.4) waits for
messages inside the VM's debug hook instead of on a thread of its own.

## Configuration

| What | Where | Who sets it |
| :- | :- | :- |
| Libraries, limits, JIT mode, and strict-mode defaults for a state | `ferrolua::Options` | The host |
| Chunk name, text or binary mode, and strict mode for one chunk | `ferrolua::ChunkOptions` | The host |
| Strict mode for one file | A first line of `--!strict` (ADR-0008) | The script's author |
| Tool settings: paths, rules, format style, taint sources and sinks | `.ferrolua.lua` (ADR-0016) | The project |
| One run of a tool | Command-line flags, which override `.ferrolua.lua` | The user |

Library crates read no environment variables and no files of their own accord. Only `ferrolua-cli`
reads them, as [`specs/cli.md`](specs/cli.md) lists.
