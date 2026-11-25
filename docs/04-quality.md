# 04 — Quality

Lunaferra's tests guard three things: that it runs its language as the specs define it, and the part
it keeps from Lua 5.5 exactly as PUC-Rio Lua 5.5.1 does; that a hostile script can't break memory
safety or the host; and that it stays within its performance budgets. This document lists the test
layers, the test tools Lunaferra writes for itself, the checks every change passes, and the review
checklists. The rules behind it are [`.claude/rules/testing.md`](../.claude/rules/testing.md), which
forbids weakening a test to make it pass and sets coverage at 100% of lines and regions.

## Test layers

| Layer | Where | What it checks |
| :- | :- | :- |
| Unit tests | `#[cfg(test)] mod tests` beside the code | Each function's outcomes and edges |
| Integration tests | Each crate's `tests/` | The crate through its public API only |
| Conformance scripts | `tests/conformance/` | Behavior Lunaferra keeps from Lua 5.5, against the output PUC-Rio Lua 5.5.1 printed for each script's Lua 5.5 twin, interpreted and with the JIT forced on |
| Extension scripts | `tests/extensions/` | Behavior with no Lua 5.5 counterpart: Python's operators, indexes, ranges, f-strings, scoping, collections, and truth, the language extensions, and the intended differences, against output written from the spec |
| Differential tests | `crates/lunaferra/tests/` and a fuzz target | The interpreter and the JIT print the same output, raise the same errors, and give the same collection log |
| Determinism tests | `crates/lunaferra/tests/` | Two runs with a fixed hash seed give the same collection log, which matches the log stored for the script, on every target ([`specs/memory.md`](specs/memory.md#determinism)) |
| Property tests | Beside the code, using `lunaferra-testkit` | Laws that hold for every input, such as "formatting doesn't change the bytecode" |
| Hostile-input tests | `tests/hostile_*.rs` in each crate that reads outside data | Empty, oversized, malformed, truncated, and deeply nested input, and each limit one past its value |
| Fuzzing | `fuzz/`, with crashes saved in `fuzz/regressions/` | Every parser and loader of outside data, for crashes and hangs |
| Benchmarks | `benches/` in each crate, and `benches/lua/` | Run time and memory against the budgets in [`00-overview.md`](00-overview.md) |
| Doc tests | The examples in doc comments | Every example in the API reference compiles and runs |

`cargo test --workspace --locked` runs every layer except fuzzing and benchmarks. It does replay
every saved fuzz input.

### Conformance scripts

PUC-Rio Lua can't run Lunaferra's scripts (ADR-0023: a language of its own, with Python's operators
and 0-based indexes), so each feature Lunaferra keeps from Lua 5.5 has a pair of scripts, named
after the feature, such as `goto.lua`:

- `tests/conformance/goto.lua`, written in Lunaferra;
- `tests/conformance/reference/goto.lua`, its twin in plain Lua 5.5, with `goto.out` beside it,
  holding what PUC-Rio Lua 5.5.1 printed for the twin.

The twins make the same calls in the same order, keep each statement on the same line so that line
numbers in messages agree, and print the same text. Where a value means something different in
the two languages, the reference script prints it in Lunaferra's terms. A Lua position `i` prints as
`i - 1`. The end `j` of a Lua range from `i` to `j` prints unchanged, because Lunaferra's range for
the same elements stops before `j`. Both scripts run under their file name alone as the chunk name,
such as `goto.lua`, so messages name the same chunk. A script without a twin, or a twin without a
script, fails the runner.

A Rust test in `crates/lunaferra/tests/conformance.rs` runs each Lunaferra script in a fresh state
with the full standard library, captures what it prints, and compares the bytes with its twin's
`.out` file. It runs each script twice: interpreted, and with the JIT compiling every function on
its first call. The second run needs the `lunaferra` crate's `jit` feature, which is off by default
(ADR-0022: the JIT is an optional feature), so CI runs the runner with the feature on.

A script's output must not depend on what Lua leaves unspecified, or the recorded output won't
repeat:

- the order `pairs` and `next` visit keys, so scripts sort keys before printing them;
- how objects print: `tostring` shows an ID for a table or a function on Lunaferra (ADR-0019:
  objects print as IDs, never addresses) but an address on PUC-Rio, so conformance scripts never
  print one, while extension scripts may;
- the time, and `math.random` without a fixed seed;
- the sign of a NaN, which PUC-Rio prints as `-nan` or `nan` by processor while Lunaferra always
  prints `nan`, and the last digits of `math` functions, which come from the C library on PUC-Rio
  and from `lunaferra-math` on Lunaferra (ADR-0027), so scripts print those with `%.12g`;
- when a value outside a loop is freed, which is earlier on Lunaferra by design (ADR-0018:
  reference counting plus a deterministic loop collector). A script that observes freeing only
  after calling `collectgarbage()` behaves as on PUC-Rio and stays in `tests/conformance/`;
  one that observes it earlier goes in `tests/extensions/`.

Never edit a `.out` file by hand. Record it again with `cargo xtask record`, and read the
difference before committing it.

## The project's own test tools

ADR-0002 (Lunaferra uses no third-party code) rules out test libraries, so Lunaferra writes these.
Each is tested like any other code.

### `lunaferra-testkit`

A dev-dependency of the other crates, never published (M0.2):

- a seeded generator of random numbers (SplitMix64 to expand the seed, xoshiro256** to generate),
  so every test that uses random input repeats exactly. Its tests check both against the output
  of the reference code that their authors publish at <https://prng.di.unimi.it/>;
- a property-test runner: it checks a property against a fixed number of generated cases, and on a
  failure, shrinks the case to a smaller one that still fails and reports the seed;
- a generator of valid Lunaferra programs (M1.9), built from the grammar in
  [`specs/syntax.md`](specs/syntax.md) and [`specs/language.md`](specs/language.md), which the
  fuzzer and the property tests share.

A generator is a function that builds a case from the random choices of a `Source`, which records
each choice. To shrink a failing case, the runner edits that record: it deletes runs of choices and
lowers each choice toward zero, then runs the generator again on the edited record. It keeps each
edit that still fails and leaves a smaller record. Every generator therefore shrinks without a
shrinking function of its own, as in Hypothesis, a property-testing library for Python, provided
that choices nearer zero give simpler cases. A property returns `true`, `false`, or a `Result`, and
a property that panics fails too.

A run checks 256 cases from seed 0 unless its `Config` says otherwise, and tries at most 10,000
edits while shrinking. Case `i` uses seed `seed + i`, so the seed of a failing case reruns that case
alone. `Config::check` returns the failure, and `Config::assert` panics with a report of the seed,
the shrunk case, and the original case, which the test harness prints.

### `lunaferra-fuzz`

The fuzzer, a program in `fuzz/` that `cargo xtask fuzz` builds and runs (M0.4). A fuzzer feeds a
target, such as the parser, a large number of inputs, and reports each one that makes the target
fail. Lunaferra's fuzzer isn't coverage-guided, because stable Rust offers no way to read coverage
counters while a program runs. It makes up for that in three ways:

- **Mutation:** each run takes an input from `fuzz/corpus/<target>/` and changes it with one to
  eight edits. An edit flips one bit, inserts one byte, or splices: it keeps the input up to a
  random point and appends the end of a corpus input from another random point. A corpus input
  close to the inputs that reach deep code does the work that coverage guidance would. A target
  without a corpus starts from the empty input.
- **Generation:** from M1.9, it will build valid programs from the grammar, which reach deep into
  the compiler and the VM.
- **Differential checks:** from M5.8, it will run each program interpreted and with the JIT forced
  on, and report any difference.

A target fails an input when it panics, crashes the process, or runs longer than the time limit,
1,000 ms unless `--time-limit` says otherwise. Run `n` of a campaign from seed `s` makes its input
from the seed `s + n`, so every run repeats, on every target. The fuzzer runs each batch of 1,000
inputs in a child process: its own program, started again with `--child`, so a crash or a hang ends
only the child. The child writes a newline to a file before each input, so the parent knows which
input was running when the child stopped, and the parent stops a child once one input runs past the
time limit. Neither process needs a thread.

At the first input that fails, the fuzzer stops and shrinks the input. It runs each smaller
candidate in a child process of its own: it deletes runs of bytes and lowers each byte toward 0,
and keeps each edit after which the input still fails in the same way (a panic, a crash, or a
hang). It saves the result as `fuzz/regressions/<target>/<hash>.bin`, where `<hash>` is the 64-bit
Fowler–Noll–Vo (FNV-1a) hash of its bytes in hex, so finding a failure again doesn't save a second
copy. Then it prints the command that replays the input in one process, where a panic shows its
location.

`cargo test` replays every saved input, each in a child process under the time limit, through the
test `every_input_saved_in_the_repository_runs_as_it_should` in `fuzz/tests/fuzzer.rs`. A saved
input of a target that checks Lunaferra's code must pass. The two fixture targets, `fixture-panic`
and `fixture-hang`, fail on purpose, to give the fuzzer's own tests something to find, and their
saved inputs must still fail as each is built to.

```bash
cargo xtask fuzz <target> --seed 0 --runs 100000
cargo xtask fuzz <target> --replay fuzz/regressions/<target>/<hash>.bin
cargo xtask fuzz --replay-saved
```

`cargo xtask fuzz` with nothing after it lists every option and target. A target is a function in
`fuzz/src/target.rs` that takes an input's bytes, and it must keep no state from one input to the
next, because a child runs many inputs, and shrinking runs one again alone.

### `lunaferra-webtest`

The WebAssembly test module of the browser harness, in `xtask/webtest/` (M3.17). Rust's test harness
can't run in a browser, so this module, built for `wasm32-unknown-unknown`, runs the conformance and
extension scripts, the saved fuzz inputs, and the stored collection logs through `lunaferra`'s API,
and reports each result to the page that loaded it.
`cargo xtask test --target wasm32-unknown-unknown` serves the module and the page from a local HTTP
server written with the standard library, opens the page in a headless browser, and collects the
results (ADR-0027: a browser harness of Lunaferra's own).

### `xtask`

Build tasks run with `cargo xtask`, written with the standard library alone:

- `cargo xtask coverage` (M0.3): builds the tests with `-C instrument-coverage` in
  `target/coverage/`, runs them, merges the profiles with `llvm-profdata` from the `llvm-tools`
  component, and reads which lines and regions ran from `llvm-cov export`. A region is a span of
  code that the compiler counts as one unit, such as the body of one `if` branch. The command
  prints each crate's lines and regions, then each line and region that no test ran, and exits with
  status 1 unless the tests ran all of them. Stable Rust can't measure branch coverage, so regions
  stand in for it. Its rules for what counts:
  - It counts each crate's files outside its `tests/`, `benches/`, and `examples/` folders, so a
    `#[cfg(test)]` module beside the code counts, and an integration test doesn't. Doc tests don't
    run, because stable Rust can't instrument them.
  - A region counts as covered when any copy of its function ran it. A generic function has a copy
    for each type it is used with, and a library's code is compiled once for its unit tests and
    once for its integration tests. `llvm-cov report` scores a function by its one copy that ran
    the most regions, so a region that only another copy ran shows as missed there. A line counts
    as covered when `llvm-cov`'s view of the whole file, which adds up the counts of every copy,
    shows that it ran.
  - Every `?` is a region of its own, which runs only when it returns an error, so each `?` needs a
    test that makes it return one.
- `cargo xtask record [script]` (M0.5): runs PUC-Rio Lua 5.5.1 on the reference scripts in
  `tests/conformance/reference/`, or on the one script named, and saves what each prints to
  standard output in a `.out` file beside it, such as `goto.out` for `goto.lua`. It reads the
  interpreter's path from `LUNAFERRA_PUC_LUA`, or finds `lua` on the `PATH`, and refuses an
  interpreter whose `lua -v` doesn't report 5.5.1. Its rules:
  - It runs each script as `lua -E goto.lua` from that folder, so Lua names the chunk `goto.lua`,
    as the twins require. `-E` makes Lua ignore `LUA_INIT`, `LUA_PATH`, and its other
    environment variables, which could change what a script prints, and each script reads an empty
    standard input.
  - A named script must be one of the `.lua` files in `tests/conformance/reference/`, so it
    refuses the Lunaferra script `tests/conformance/goto.lua`.
  - It saves the `.out` files only after Lua has run every script and exited with status 0 each
    time. If any script stops with an error, it saves no `.out` file, and Lua's message appears on
    standard error.
  - It prints one line for each `.out` file, which says whether the file is `new`, `changed`, or
    `unchanged`, and leaves an unchanged file as it is.
- `cargo xtask fuzz [arguments]` (M0.4): builds `lunaferra-fuzz` and runs it with the arguments
  after `fuzz`, as the section above describes, and exits with its status. Unlike the other tasks,
  it leaves the report to the fuzzer.
- `cargo xtask test --target <triple>` (M3.17): runs the tests on any target in
  [`03-platforms.md`](03-platforms.md#how-tests-run-on-each-target), natively, in the Android
  emulator, in the iOS simulator, or in a headless browser through a harness of Lunaferra's own.
- `cargo xtask bench` (M5.1): runs each script in `benches/lua/` on Lunaferra, and its Lua 5.5
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

The last command must list only Lunaferra's own crates. Once M7 lands, `lunaferra fmt --check` and
`lunaferra lint` also check every Lunaferra script in `tests/` and `benches/`, skipping the Lua 5.5
twins in `reference/` folders.

## Continuous integration

CodeQL already scans the workflows and the Rust code on every push and pull request to `main`, and
once a week (`.github/workflows/codeql.yml`). Dependabot proposes each new Rust release for
`rust-toolchain.toml` as a pull request (`.github/dependabot.yml`). The CI workflow,
`.github/workflows/ci.yml`, lands in M0.6. It runs every automated check above on `ubuntu-latest`,
plus a short fuzz run from a fixed seed. From M3.18 on, a job for each target in
[`03-platforms.md`](03-platforms.md#targets) runs `cargo xtask test` there, and coverage runs on
Linux, Windows, and macOS (ADR-0027: six platforms); from M13.6 on, each JIT target also runs the
forced pass and a short differential fuzz run. The owner then makes each job required before a merge
to `main`. CI never installs PUC-Rio Lua: it compares against the recorded `.out` files.

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
