# 04 — Quality

Ferrolua's tests guard three things: that it runs its language as the specs define it, and the part
it keeps from Lua 5.5 exactly as PUC-Rio Lua 5.5.1 does; that a hostile script can't break memory
safety or the host; and that it stays within its performance budgets. This document lists the test
layers, the test tools Ferrolua writes for itself, the checks every change passes, and the review
checklists. The rules behind it are [`.claude/rules/testing.md`](../.claude/rules/testing.md), which
forbids weakening a test to make it pass and sets coverage at 100% of lines and regions.

## Test layers

| Layer | Where | What it checks |
| :- | :- | :- |
| Unit tests | `#[cfg(test)] mod tests` beside the code | Each function's outcomes and edges |
| Integration tests | Each crate's `tests/` | The crate through its public API only |
| Conformance scripts | `tests/conformance/` | Behavior Ferrolua keeps from Lua 5.5, against the output PUC-Rio Lua 5.5.1 printed for each script's Lua 5.5 twin, interpreted and with the JIT forced on |
| Extension scripts | `tests/extensions/` | Behavior with no Lua 5.5 counterpart: Python's operators, indexes, ranges, f-strings, scoping, collections, and truth, the language extensions, and the intended differences, against output written from the spec |
| Differential tests | `crates/ferrolua/tests/` and a fuzz target | The interpreter and the JIT print the same output, raise the same errors, and give the same collection log |
| Determinism tests | `crates/ferrolua/tests/` | Two runs with a fixed hash seed give the same collection log, which matches the log stored for the script, on every target ([`specs/memory.md`](specs/memory.md#determinism)) |
| Property tests | Beside the code, using `ferrolua-testkit` | Laws that hold for every input, such as "formatting doesn't change the bytecode" |
| Hostile-input tests | `tests/hostile_*.rs` in each crate that reads outside data | Empty, oversized, malformed, truncated, and deeply nested input, and each limit one past its value |
| Fuzzing | `fuzz/`, with crashes saved in `fuzz/regressions/` | Every parser and loader of outside data, for crashes and hangs |
| Benchmarks | `benches/` in each crate, and `benches/lua/` | Run time and memory against the budgets in [`00-overview.md`](00-overview.md) |
| Doc tests | The examples in doc comments | Every example in the API reference compiles and runs |

`cargo test --workspace --locked` runs every layer except fuzzing and benchmarks. It does replay
every saved fuzz input.

### Conformance scripts

PUC-Rio Lua can't run Ferrolua's scripts (ADR-0023: a language of its own, with Python's operators
and 0-based indexes), so each feature Ferrolua keeps from Lua 5.5 has a pair of scripts, named
after the feature, such as `goto.lua`:

- `tests/conformance/goto.lua`, written in Ferrolua;
- `tests/conformance/reference/goto.lua`, its twin in plain Lua 5.5, with `goto.out` beside it,
  holding what PUC-Rio Lua 5.5.1 printed for the twin.

The twins make the same calls in the same order, keep each statement on the same line so that line
numbers in messages agree, and print the same text. Where a value means something different in
the two languages, the reference script prints it in Ferrolua's terms. A Lua position `i` prints as
`i - 1`. The end `j` of a Lua range from `i` to `j` prints unchanged, because Ferrolua's range for
the same elements stops before `j`. Both scripts run under their file name alone as the chunk name,
such as `goto.lua`, so messages name the same chunk. A script without a twin, or a twin without a
script, fails the runner.

A Rust test in `crates/ferrolua/tests/conformance.rs` runs each Ferrolua script in a fresh state
with the full standard library, captures what it prints, and compares the bytes with its twin's
`.out` file. It runs each script twice: interpreted, and with the JIT compiling every function on
its first call. The second run needs the `ferrolua` crate's `jit` feature, which is off by default
(ADR-0022: the JIT is an optional feature), so CI runs the runner with the feature on.

A script's output must not depend on what Lua leaves unspecified, or the recorded output won't
repeat:

- the order `pairs` and `next` visit keys, so scripts sort keys before printing them;
- how objects print: `tostring` shows an ID for a table or a function on Ferrolua (ADR-0019:
  objects print as IDs, never addresses) but an address on PUC-Rio, so conformance scripts never
  print one, while extension scripts may;
- the time, and `math.random` without a fixed seed;
- when a value outside a loop is freed, which is earlier on Ferrolua by design (ADR-0018:
  reference counting plus a deterministic loop collector). A script that observes freeing only
  after calling `collectgarbage()` behaves as on PUC-Rio and stays in `tests/conformance/`;
  one that observes it earlier goes in `tests/extensions/`.

Never edit a `.out` file by hand. Record it again with `cargo xtask record`, and read the
difference before committing it.

## The project's own test tools

ADR-0002 (Ferrolua uses no third-party code) rules out test libraries, so Ferrolua writes these.
Each is tested like any other code.

### `ferrolua-testkit`

A dev-dependency of the other crates, never published (M0.2):

- a seeded generator of random numbers (SplitMix64 to expand the seed, xoshiro256** to generate),
  so every test that uses random input repeats exactly;
- a property-test runner: it checks a property against a fixed number of generated cases, and on a
  failure, shrinks the case to a smaller one that still fails and prints the seed;
- a generator of valid Ferrolua programs, built from the grammar in
  [`specs/syntax.md`](specs/syntax.md) and [`specs/language.md`](specs/language.md), which the
  fuzzer and the property tests share.

### `ferrolua-fuzz`

The fuzzer, in `fuzz/` (M0.4). It isn't coverage-guided, because stable Rust offers no way to read
coverage counters while a program runs. It makes up for that in three ways:

- **Generation:** it builds valid programs from the grammar, which reach deep into the compiler and
  the VM.
- **Mutation:** it flips, inserts, and splices bytes in the inputs in `fuzz/corpus/<target>/`.
- **Differential checks:** it runs each program interpreted and with the JIT forced on, and reports
  any difference.

Each run starts from a fixed seed, so a run repeats. It runs each batch of inputs in a child
process with a time limit, so a hang is caught without a thread. When a target crashes, the fuzzer
shrinks the input, saves it as `fuzz/regressions/<target>/<hash>.bin`, and prints the command to
replay it. `cargo test` replays every saved input.

```bash
cargo xtask fuzz <target> --seed 0 --runs 100000
```

### `xtask`

Build tasks run with `cargo xtask`, written with the standard library alone:

- `cargo xtask coverage` (M0.3): builds the tests with `-C instrument-coverage`, merges the
  profiles with `llvm-profdata` from the `llvm-tools` component, reports with `llvm-cov`, and fails
  when lines or regions fall below 100%. Stable Rust can't measure branch coverage, so regions
  stand in for it.
- `cargo xtask record [script]` (M0.5): runs PUC-Rio Lua 5.5.1 on the reference scripts in
  `tests/conformance/reference/`, from that folder, and writes their `.out` files. It refuses an
  interpreter whose `lua -v` doesn't report 5.5.1. It reads the interpreter's path from
  `FERROLUA_PUC_LUA`, or finds `lua` on the `PATH`.
- `cargo xtask fuzz` (M0.4): runs the fuzzer.
- `cargo xtask bench` (M5.1): runs each script in `benches/lua/` on Ferrolua, and its Lua 5.5
  twin in `benches/lua/reference/` on PUC-Rio Lua 5.5.1, and reports the median of several runs.

## Automated checks

Every change passes these, locally and in CI, with the same flags:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo xtask coverage
cargo tree --workspace --edges normal,build,dev --target all --prefix none
```

The last command must list only Ferrolua's own crates. Once M7 lands, `ferrolua fmt --check` and
`ferrolua lint` also check every Ferrolua script in `tests/` and `benches/`, skipping the Lua 5.5
twins in `reference/` folders.

## Continuous integration

CodeQL already scans the workflows on every push and pull request to `main`
(`.github/workflows/codeql.yml`). The CI workflow, `.github/workflows/ci.yml`, lands in M0.6. It
runs every automated check above on `ubuntu-latest`, plus a short fuzz run from a fixed seed. The
owner then makes each job required before a merge to `main`. CI never installs PUC-Rio Lua: it
compares against the recorded `.out` files.

## Review checklists

**Every change:**

- The tests came with the code, assert exact results, and pass alone and in any order.
- No test was weakened, skipped, or deleted, and coverage is still 100%.
- The spec, the roadmap, and the status locations agree with the code, as the update map in
  [`CONTRIBUTING.md`](../CONTRIBUTING.md#documentation) lists.

**`unsafe` code:**

- It is inside a module that an accepted decision record allows (ADR-0013).
- Each block has a `// SAFETY:` comment that names the invariant it relies on.
- A test exercises it, and a test checks each invariant it relies on.

**Behavior kept from Lua 5.5:**

- A conformance script and its Lua 5.5 twin cover the change, with output recorded from PUC-Rio
  Lua 5.5.1, and the review reads the two side by side.
- An intended difference from PUC-Rio names its decision record, and its script is in
  `tests/extensions/`.

**The language:**

- The precedence property test still passes (M1.5).
- [`specs/language.md`](specs/language.md) describes the change, its decision record is accepted,
  and a script in `tests/extensions/` covers it, with output written from the spec.

**Parsers, loaders, and limits:**

- Hostile-input tests cover the new path, a fuzz target reaches it, and each limit has a test one
  past its value.

**Diagnostics:**

- A new diagnostic takes the next free code, and no code is renamed or reused
  ([`specs/diagnostics.md`](specs/diagnostics.md)).

**Public API:**

- Each public item has a doc comment with an example, and a breaking change names its decision
  record.
