# Roadmap

This file is the plan for building Lunaferra, from an empty workspace to the first release on
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
- Within each milestone, work happens on Linux on x86-64. From M3.18 on, CI also builds and tests
  every target in [`03-platforms.md`](03-platforms.md), and a task's checks pass on each.
- A task that needs PUC-Rio Lua 5.5.1 to record output, on a machine without it, leaves its box
  unticked with a `⏳ needs PUC-Rio Lua 5.5.1` line naming the check that remains.
- A conformance script that a task adds comes with its Lua 5.5 twin in
  `tests/conformance/reference/` and the twin's recorded `.out` file, as
  [`04-quality.md`](04-quality.md#conformance-scripts) describes.

## M0 — A workspace where every check runs

**Goal:** A contributor can clone the repository and run every check, including coverage, the
fuzzer, and the conformance recorder, on an empty workspace, locally and in CI.

- [x] **M0.1 — workspace: create the Cargo workspace**
  - Note: Built on 2026-11-24 with Rust 1.100.0 (rustc `0ef1bc386`, 2026-11-09), Cargo 1.100.0, and
    `resolver = "3"`. The 21 members follow the crate map's order, and no crate depends on another
    yet: each task adds the dependencies its code needs.
  - Note: `[workspace.lints]` holds the lints `.claude/rules/rust.md` names (`unsafe_code` forbidden,
    `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks` denied, and `missing_docs`,
    `missing_debug_implementations`, and `unreachable_pub`), plus Clippy lints for the rule's other
    points: `allow_attributes`, `allow_attributes_without_reason`, `unwrap_used`, `dbg_macro`,
    `todo`, `unimplemented`, `print_stdout`, `print_stderr`, and four `cast_*` lints. `clippy.toml`
    allows `unwrap` in tests, but not in a helper function in a crate's `tests/` folder, which uses
    `expect` instead.
  - Note: No crate sets `version`, so Cargo gives each one 0.0.0 and won't publish it.
  - Note: Step 3 of the definition of done, coverage, couldn't run, because M0.3 builds
    `cargo xtask coverage`. Until then, `cargo xtask` runs an empty `main` and exits with status 0
    whatever it is given.
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
    - The tree command lists only Lunaferra's own crates.
  - Verify: `cargo build --workspace --locked`;
    `cargo tree --workspace --edges normal,build,dev --target all --prefix none` (expect only
    `lunaferra` crates and `xtask`)

- [x] **M0.2 — testkit: seeded generator and property tests**
  - Note: Built on 2026-11-25. The xoshiro authors publish reference C code, not a list of outputs,
    so the expected numbers in `crates/testkit/tests/random.rs` come from `splitmix64.c` and
    `xoshiro256starstar.c` at <https://prng.di.unimi.it/>, compiled with GCC 13.3.0 on x86-64 Linux.
    From the state `{1, 2, 3, 4}`, xoshiro256** gives `11520`, `0`, and `1509978240` first. Seeded
    with 0, which fills the state from SplitMix64, it gives `11091344671253066420` first.
  - Note: The runner shrinks the record of a generator's random choices, not the case itself, so no
    generator needs a shrinking function of its own. A run checks 256 cases from seed 0 by default,
    tries at most 10,000 edits while shrinking, and gives case `i` the seed `seed + i`. The crate
    never prints: `Config::assert` panics with a report that holds the seed, and the test harness
    prints the report.
  - Note: The property `x <= 1000`, on numbers below `u64::MAX` from seed 0, fails on its first case,
    `11091344671253066419`, which shrinks to `1001` after 61 smaller failing cases.
  - Note: Step 3 of the definition of done, coverage, couldn't run as written, because M0.3 builds
    `cargo xtask coverage`. Measured by hand with `-C instrument-coverage`, `llvm-profdata`, and
    `llvm-cov report` from the `llvm-tools` component, the crate's tests cover all 235 of its lines
    and all 236 of its regions.
  - Depends: M0.1
  - Do:
    - Build `lunaferra-testkit` as [`04-quality.md`](04-quality.md#lunaferra-testkit) describes: a
      SplitMix64 seed expander, xoshiro256**, and a property runner that shrinks a failing case and
      prints its seed.
  - Accept:
    - The same seed gives the same sequence on every run, matching the reference outputs that the
      xoshiro256** authors publish.
    - A property that fails for values above 1,000 is reported with a shrunk case of 1,001.
  - Verify: `cargo test -p lunaferra-testkit --locked`

- [x] **M0.3 — xtask: coverage command**
  - Note: Built on 2026-11-25. `cargo xtask coverage` runs `cargo test --workspace --locked --tests`
    with `RUSTFLAGS="-C instrument-coverage"` in `target/coverage/`, twice: once with `--no-run`,
    to list the programs Cargo built, and once to run them. It merges the profiles with
    `llvm-profdata merge -sparse`, then reads the regions from `llvm-cov export -format=text` and the
    lines from `llvm-cov export -format=lcov`. It finds both tools from
    `rustc --print sysroot --print host-tuple`, in `lib/rustlib/<host>/bin/`. It reads Cargo's and
    `llvm-cov`'s JSON with a reader of its own, in `xtask/src/json.rs`.
  - Note: It takes the union of every instantiation, the second option in M0.2's note: a region
    counts as covered when any copy of its function ran it, and a line when `llvm-cov`'s view of
    the whole file, which adds up every copy, shows that it ran. In the fixture of
    `xtask/tests/coverage.rs`, whose tests run each branch of a generic function in a different
    copy, `llvm-cov report` gives the function 4 of its 5 regions, and the command gives all 5.
  - Note: It counts each crate's files outside `tests/`, `benches/`, and `examples/`, the same
    files M0.2 measured by hand. The owner confirmed this rule on 2026-11-25. Integration tests can
    hold code that never runs by design, such as the end of a `#[should_panic]` test, or a
    generator that a run of 0 cases never calls, as in `crates/testkit/tests/property.rs`. Doc
    tests don't run under it, because stable Rust can't instrument them. Build scripts aren't
    measured either, and no crate has one yet.
  - Note: On 2026-11-25, on this machine (x86-64 Linux, Rust 1.100.0), a run from an empty
    `target/coverage/` took 2.1 s and covered all 1,691 lines and 1,986 regions, in `lunaferra-cli`,
    `lunaferra-testkit`, and `xtask`. `crates/cli/tests/cli.rs` runs the `lunaferra-cli` binary, and
    `xtask/tests/coverage.rs` runs the `xtask` binary, so each `main` runs under the tests.
  - Note (from M0.1): `crates/cli/src/main.rs` and `xtask/src/main.rs` each hold an empty
    `fn main` that no test calls, so the coverage run has to reach both, for example by running
    each binary from an integration test.
  - Note (from M0.2): `llvm-cov report` scores a generic function by its one instantiation that
    covers the most regions, not by the union of all its instantiations. A generic function that
    tests call with different closures can therefore show a missed region that a test does run.
    `lunaferra-testkit` reaches 100% on that measure because its tests share named generator and
    property functions. The coverage command should either keep that measure, or take the union of
    every instantiation's regions from `llvm-cov export`.
  - Depends: M0.1
  - Do:
    - Build `cargo xtask coverage` as [`04-quality.md`](04-quality.md#xtask) describes, finding
      `llvm-profdata` and `llvm-cov` under `rustc --print sysroot`.
  - Accept:
    - It prints line and region coverage for each crate, and fails below 100%.
    - Its own test, on a fixture crate with one uncovered function, sees it fail.
  - Verify: `cargo xtask coverage` (expect 100% of lines and regions); `cargo test -p xtask --locked`

- [x] **M0.4 — fuzz: the fuzzer and regression replay**
  - Note: Built on 2026-11-25. `lunaferra-fuzz` is a program, as `xtask` is, and `cargo xtask fuzz`
    runs it with `cargo run --locked --package lunaferra-fuzz`, in Cargo's dev profile. It sends
    1,000 runs to each child process, and gives each input 1,000 ms unless `--time-limit` says
    otherwise. The parent hands a child its batch, and reads its progress, through two files in a
    work folder, `lunaferra-fuzz-<process ID>` in the system's temporary folder, which it removes
    when it ends. On this machine (x86-64 Linux, an Intel Core i7-11800H, Rust 1.100.0), one
    million runs of `fixture-panic` from an empty corpus take 8.3 s.
  - Note: Without coverage guidance, a fuzzer doesn't find a 4-byte prefix from nothing, so
    `fixture-panic`'s corpus, `fuzz/corpus/fixture-panic/fuz.bin`, holds the text
    `FUZ, one byte short of what fixture-panic panics on`. From seed 0, run 676 splices it into a
    52-byte input that starts with `FUZZ`, which shrinks to `FUZZ`, saved as
    `fuzz/regressions/fixture-panic/c6d0a3857a0d9820.bin`. The command exits with status 1, because
    a run failed.
  - Note: `fixture-hang` follows links through its input, where each byte is the index of the next,
    with no bound on the steps. From seed 0 and no corpus, run 1750 hangs on a 3-byte input, which
    shrinks to the single byte 0. Shrinking a hang costs the time limit for each smaller input that
    still hangs, so the whole run takes about 3 s at 1,000 ms. A child that the runner stops never
    writes its coverage profile, so every line of the fixture also runs on inputs that pass.
  - Note: The saved `FUZZ` input stays in `fuzz/regressions/`, so `cargo test` replays it. Each
    target says how its saved inputs must replay: a fixture's must still fail as it is built to,
    and the input of a target that checks Lunaferra's code must pass.
  - Depends: M0.2
  - Do:
    - Build `lunaferra-fuzz` in `fuzz/` as [`04-quality.md`](04-quality.md#lunaferra-fuzz)
      describes: targets, mutation, a fixed seed, child processes with a time limit, shrinking, and
      saving to `fuzz/regressions/`.
    - Add `cargo xtask fuzz`, and a test that replays every saved input.
  - Accept:
    - A fixture target that panics on inputs starting with `FUZZ` is found from seed 0, shrunk to
      those 4 bytes, saved, and then replayed by `cargo test`.
    - A fixture target that loops forever is reported as a hang within its time limit.
  - Verify: `cargo xtask fuzz fixture-panic --seed 0 --runs 100000` (expect one saved input);
    `cargo test -p lunaferra-fuzz --locked`

- [x] **M0.5 — xtask: conformance recorder**
  - Note: Built on 2026-11-25. `cargo xtask record` finds the workspace's root with
    `cargo metadata`, as `cargo xtask coverage` does, then runs `lua -E -v`, and `lua -E <name>` on
    each script, from `tests/conformance/reference/`. `-E` makes Lua ignore `LUA_INIT` and its other
    environment variables. It accepts an interpreter whose first line of output from `-v` is
    `Lua 5.5.1`, alone or followed by whitespace, so it refuses `Lua 5.5.10` and LuaJIT. It saves
    the `.out` files only after every script exits with status 0, and prints `new`, `changed`, or
    `unchanged` for each one. The 5.5.1 sources on lua.org, read on 2026-11-25, keep `-E`, and
    their `lua.h` defines the line that `-v` prints as
    `Lua 5.5.1  Copyright (C) 1994-2026 Lua.org, PUC-Rio`.
  - Note: CI never installs PUC-Rio Lua, so `xtask/tests/record.rs` runs the task with a stand-in
    interpreter, a Rust program that the tests compile with `rustc`. The tests check that it
    refuses Lua 5.4.7, an interpreter that prints no version, and one whose `-v` fails, and that
    recording a script twice reports its `.out` file `unchanged` the second time.
  - Note: On 2026-11-25, with the owner's go-ahead, PUC-Rio Lua 5.5.1 was built in a scratch folder
    with `make linux` and GCC 13.3.0, from `lua-5.5.1.tar.gz` on lua.org: 398,643 bytes, SHA-256
    `1c4b4068d67061f2a2231ad2b5422e77acea1487ea9890f6320af614f4373dce`, as lua.org lists. Its
    `lua -v` printed the line above. `cargo xtask record tests/conformance/reference/hello.lua`
    saved `hello.out`, the 13 bytes `hello, world` and a newline, and reported it `new`. A second
    run, and a run with no script named, reported it `unchanged`.
  - Depends: M0.1
  - Do:
    - Build `cargo xtask record`, which runs PUC-Rio Lua 5.5.1 on the reference scripts in
      `tests/conformance/reference/` and writes their `.out` files, as
      [`04-quality.md`](04-quality.md#xtask) describes.
    - Add `tests/conformance/hello.lua`, which prints one line, its twin
      `tests/conformance/reference/hello.lua`, and the twin's recorded `.out` file.
  - Accept:
    - It refuses, with a clear message, an interpreter whose `lua -v` doesn't report Lua 5.5.1.
    - Recording a script twice gives the same `.out` file.
  - Verify: `cargo xtask record tests/conformance/reference/hello.lua`;
    `cargo test -p xtask --locked`

- [x] **M0.6 — ci: the CI workflow**
  - Note: Written on 2026-11-25. `.github/workflows/ci.yml` runs seven jobs on `ubuntu-latest`, one
    for each automated check of [`04-quality.md`](04-quality.md#automated-checks) and one that
    fuzzes: Format, Clippy, Test, Documentation, Coverage, Dependencies, and Fuzz. Each job checks
    out the code with `actions/checkout` v7.0.1, the newest release that `git ls-remote` listed on
    2026-11-25, pinned to its commit `3d3c42e5aac5ba805825da76410c181273ba90b1`. It then runs
    `rustup toolchain install`, which with no toolchain named installs the one that
    `rust-toolchain.toml` pins, with its components, as rustup 1.29.3's help says. Each job has only
    `contents: read`, runs its steps in `bash` with `-e` and `-o pipefail`, and fails if it runs
    longer than 30 minutes.
  - Note: The fuzzer has two new options for the Fuzz job. `--seconds <n>` stops a campaign once
    `n` seconds have passed, checking the time before each batch of 1,000 runs, and without
    `--runs` it sets no limit on the runs. On this machine (x86-64 Linux, an Intel Core i7-11800H),
    `cargo xtask fuzz fixture-panic --seconds 5` with an empty corpus made 2,438,000 runs in 5.04 s.
    `--list` prints each target whose saved inputs replay as `Kind::Passed`, which leaves out the
    fixtures. The job runs `cargo xtask fuzz <target> --seed 0 --seconds 60` on each, so it fuzzes
    nothing until M1.9. With a stand-in `--list` that printed `fixture-panic`, the job's script
    stopped with status 1 at the panic in run 676, and `--seed 676 --runs 1` found it again.
  - Note: The Dependencies job fails when `cargo tree` prints a line that is neither empty nor a
    crate named `lunaferra`, `lunaferra-<part>`, or `xtask` with a path under `$GITHUB_WORKSPACE`.
    Run on this workspace, its script passed. On a made-up list with a crates.io package and a Git
    package, it printed those two lines and stopped with status 1.
  - Note: Checked on GitHub on 2026-11-25, in pull request #2. On `d64cbbf`, CI run 45705186779
    passed all seven jobs with `rustc 1.100.0`: Test ran 268 tests with none failed or ignored,
    Coverage reported all 3,573 lines and 4,187 regions covered, and Fuzz printed
    `No fuzz target checks Lunaferra's code yet.` Commit `7cbc896` removed the spaces in one closure
    in `fuzz/src/target.rs`, and run 45736347594 failed the Format job with that diff while the
    other six passed. Its revert, `6c94d5d`, passed run 45736497912. The owner then made the seven
    jobs required checks from GitHub Actions in the `BestBranch` ruleset, which GitHub's rules
    API listed for `main`.
  - Note (from M0.4): The fuzzer stops after `--runs` runs, and has no option for a length of time,
    so the run of 60 s per target needs one, such as `--seconds`, or a number of runs measured to
    take about 60 s. `cargo xtask fuzz` exits with status 1 when a run fails, which fails the job.
    The two fixture targets fail on purpose, so CI fuzzes only the others, and there are none
    until M1.9.
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

## M1 — Parse Lunaferra, with errors that point at the problem

**Goal:** `lunaferra parse` reads any Lunaferra file, reports syntax errors in the words PUC-Rio
Lua 5.5.1 uses wherever the syntax is Lua's, and prints a lossless tree, which every later tool
builds on.

- [x] **M1.1 — syntax: source text, spans, and line maps**
  - Note: Built on 2026-11-25. `crates/syntax/src/source.rs` holds `SourceId`, with `new` and
    `index`; `Span`, with public fields and its end excluded; `LineMap`, with `new` and `line_col`;
    and `MAX_SOURCE_LEN`. `crates/syntax/src/error.rs` holds the crate's error enum, `Error`, whose
    one variant so far is `SourceTooLong`.
  - Note: `MAX_SOURCE_LEN` is `u32::MAX - 1` bytes, one byte less than the spec first said. In a
    source of `u32::MAX` bytes with no line break, the column just past the end would be `2^32`,
    which a `u32` can't hold. `line_col` gives an offset past the end the position of the end, so
    it never panics.
  - Note: The line counting follows `inclinenumber` in Lua 5.5.1's `llex.c`, read on lua.org on
    2026-11-25. `crates/syntax/tests/source.rs` checks the line map against a model of it on all
    21,845 sources of up to seven bytes made from `a`, `\n`, `\r`, and 0xFF. The project has no
    mutation tool, so the change was checked by hand: each of nine edits to `source.rs`, such as
    pairing `\n\n`, dropping the clamp at the end, or moving the length limit, failed at least one
    test.
  - Depends: M0.1
  - Do:
    - Build `Span`, `SourceId`, and `LineMap`, as [`specs/syntax.md`](specs/syntax.md#source-and-spans)
      describes.
  - Accept:
    - Line and column are right for every line ending PUC-Rio counts, and for sources that aren't
      valid UTF-8.
  - Verify: `cargo test -p lunaferra-syntax --locked`

- [ ] **M1.2 — syntax: the lexer**
  - Note: 2026-11-24, the owner's choice (ADR-0023): the lexer reads Python's operators instead of
    `..` and `~=`.
  - Note: 2026-11-24, the owner's choice (ADR-0025): the lexer also reads f-strings.
  - Note: 2026-11-24, the owner accepted ADR-0026 (the details of f-strings, locals by default,
    collections, and truth), which sets the `f` prefix and keeps `[[` a long string.
  - Note (from M1.1): `LineMap::new` already rejects a source longer than `MAX_SOURCE_LEN` with
    `Error::SourceTooLong`, so a lexer that builds the line map first needn't check the length
    again. In Lua 5.5.1, `inclinenumber` stops with `chunk has too many lines` when the line
    number reaches `INT_MAX`. `luaL_loadfilex`, in `lauxlib.c`, skips a UTF-8 byte order mark, then
    a first line that starts with `#` up to the first `\n` only, and puts a `\n` in its place. So a
    lone `\r` in that line doesn't end a line for PUC-Rio, though `LineMap` counts it, and how the
    lexer skips the line decides which count Lunaferra keeps.
  - Depends: M1.1
  - Do:
    - Build the lexer of [`specs/syntax.md`](specs/syntax.md#lexer), with trivia on every token.
    - Add `tests/conformance/lexer.lua`, covering numbers, escapes, and long strings, with its
      recorded output.
  - Accept:
    - Every token form of §3.1 of the manual lexes, apart from `..` and `~=`, and so do `**`, `!=`,
      and the reserved word `is`. Every malformed form gives PUC-Rio's error, such as
      `malformed number near '0x'`.
    - `f"a{x}b"`, `f'{x!r:>8}'`, and `f[[{x}]]` lex as f-strings with their fields, `{{` as a brace,
      and `f "x"` as a name and a string.
  - Verify: `cargo test -p lunaferra-syntax --locked`

- [ ] **M1.3 — syntax: parser events and the lossless tree**
  - Depends: M1.2
  - Do:
    - Build the event stream and the lossless tree builder, with typed views for each node kind.
  - Accept:
    - Printing the tree gives back the source, byte for byte.
  - Verify: `cargo test -p lunaferra-syntax --locked`

- [ ] **M1.4 — syntax: statements**
  - Depends: M1.3
  - Do:
    - Parse every statement of §3.3 and §9 of the manual, including `global` declarations in every
      form, attributes, `goto` and labels, and named varargs.
  - Accept:
    - Each statement form parses to its node kind, and each nearby error has a test with its message.
  - Verify: `cargo test -p lunaferra-syntax --locked`

- [ ] **M1.5 — syntax: expressions and operator precedence**
  - Note: 2026-11-24, the owner's choice (ADR-0023): expressions follow Python's operators and
    precedence, with chained comparisons, conditional expressions, `in`, and `is`.
  - Note: 2026-11-24, the owner accepted ADR-0024 (the details Python's operators and 0-based
    indexes leave open), which makes `is` a reserved word.
  - Depends: M1.3
  - Do:
    - Parse expressions with the precedence, grouping, and grammar of
      [`specs/language.md`](specs/language.md#precedence), including chained comparisons,
      conditional expressions, `in`, `not in`, `is`, and `is not`, plus table constructors,
      function definitions, calls, and method calls.
    - Report `..`, `~=`, and binary `~` with messages that name their replacements, and record the
      messages in the language spec.
    - Add the precedence property test of [`specs/language.md`](specs/language.md#tests).
    - Parse list and tuple literals and f-string fields, as
      [`specs/language.md`](specs/language.md#lists-tuples-and-sets-adr-0025) and
      [`specs/language.md`](specs/language.md#formatted-string-literals-adr-0025) describe
      (ADR-0025).
  - Accept:
    - `2 ** -3 ** 2`, `-x ** 2`, `not a == b`, `a < b < c`, `1 + 2 << 3`, and
      `x if a else y if b else z` group as the precedence table says.
    - `x = f if c then g() end` parses as an assignment followed by an `if` statement.
    - `(x)` parses as a parenthesized expression, `(x,)` and `()` as tuples, `[ [1], [] ]` as a list
      of lists, and `f"{p:length()}"` as a method call where `f"{x:>8}"` has a spec.
  - Verify: `cargo test -p lunaferra-syntax --locked`

- [ ] **M1.6 — syntax: error recovery and the diagnostic type**
  - Depends: M1.4, M1.5
  - Do:
    - Build the `Diagnostic` type of [`specs/diagnostics.md`](specs/diagnostics.md#the-diagnostic),
      and recovery as [`specs/syntax.md`](specs/syntax.md#parser-and-events) describes.
    - Add `tests/conformance/syntax_errors.lua`, which loads chunks that are broken alike in
      Lunaferra and Lua 5.5 and prints each message.
  - Accept:
    - A file with three separate errors gets three diagnostics, and the statements between them
      still appear in the tree.
    - `load`'s message for each broken chunk matches PUC-Rio's.
  - Verify: `cargo test -p lunaferra-syntax --locked`

- [ ] **M1.7 — syntax: compact tree for the compiler**
  - Depends: M1.6
  - Do:
    - Build the compact tree from the same events, as [`specs/syntax.md`](specs/syntax.md#compact-tree)
      describes.
  - Accept:
    - For generated valid programs, the compact tree matches the lossless tree's structure.
  - Verify: `cargo test -p lunaferra-syntax --locked`

- [ ] **M1.8 — syntax: size and nesting limits**
  - Depends: M1.6
  - Do:
    - Enforce the source size and nesting limits of [`specs/sandbox.md`](specs/sandbox.md#limits),
      with hostile-input tests in `crates/syntax/tests/hostile_syntax.rs`.
  - Accept:
    - Nesting at PUC-Rio Lua 5.5.1's limit parses, and one level more stops with PUC-Rio's message,
      both recorded in the syntax spec.
    - A 16 MB file parses in time proportional to its size.
  - Verify: `cargo test -p lunaferra-syntax --test hostile_syntax --locked`

- [ ] **M1.9 — fuzz: lexer and parser targets**
  - Note (from M0.2): A generator in `lunaferra-testkit` is a function of `&mut Source` that makes
    each choice with `Source::below`. The program generator shrinks well if a choice of 0 gives the
    simplest program, for example when alternative 0 of each grammar rule is its shortest. Only the
    property runner can make a `Source` today, so the fuzzer needs a way to make one as well.
  - Note (from M0.4): A target is an entry in `TARGETS` in `fuzz/src/target.rs`, with
    `replays_as: Kind::Passed`, and its corpus goes in `fuzz/corpus/<target>/`. The fuzzer makes
    each run's input only by mutation, in `fuzz/src/campaign.rs`, which is where generation from
    the run's seed joins it. The fuzzer runs in Cargo's dev profile, which keeps overflow checks but
    doesn't optimize; whether a million parser runs need a profile that does both is open.
  - Note (from M0.6): CI's Fuzz job runs `cargo xtask fuzz <target> --seed 0 --seconds 60` on each
    target that `cargo xtask fuzz --list` prints, so the `lexer` and `parser` targets join it with no
    change to `.github/workflows/ci.yml`. The test `lists_no_target_while_only_the_fixtures_exist`
    in `fuzz/tests/fuzzer.rs` expects `--list` to print nothing, so this task changes it to expect
    the two names.
  - Depends: M0.4, M1.7
  - Do:
    - Add the Lunaferra program generator to `lunaferra-testkit`, and the `lexer` and `parser`
      targets, which check the round trip and that nothing panics.
  - Accept:
    - One million runs of each target from seed 0 find no crash, and every input they ever crashed on
      is saved.
  - Verify: `cargo xtask fuzz parser --seed 0 --runs 1000000` (expect no new saved input)

- [ ] **M1.10 — cli: `lunaferra parse`**
  - Note (from M0.1): Naming the binary `lunaferra` makes `cargo doc --workspace` warn of an output
    filename collision with the `lunaferra` library
    ([Cargo issue 6313](https://github.com/rust-lang/cargo/issues/6313)). Setting `doc = false` on
    the `[[bin]]` target in `crates/cli/Cargo.toml` removes the warning, as a scratch workspace
    showed on 2026-11-24 with Cargo 1.100.0.
  - Depends: M1.6
  - Do:
    - Build the `lunaferra` binary's argument parser and the `parse` command of
      [`specs/cli.md`](specs/cli.md#commands).
  - Accept:
    - A valid file exits with 0, a broken one prints its diagnostics and exits with 1, and a missing
      file exits with 2.
  - Verify: `cargo test -p lunaferra-cli --locked`

**Exit criteria (M1):** `lunaferra parse` handles every Lunaferra script in `tests/conformance/`,
reports errors in PUC-Rio's words wherever the syntax is Lua's, and the fuzz targets run clean.

---

## M2 — Run Lunaferra scripts in the interpreter

**Goal:** `lunaferra run` runs scripts that use the core language and the base library, printing
what PUC-Rio Lua 5.5.1 prints for their Lua 5.5 twins, with memory freed by reference counting.

- [ ] **M2.1 — bytecode: instructions and prototypes**
  - Depends: M0.1
  - Do:
    - Define the instruction words and `Prototype` of [`specs/bytecode.md`](specs/bytecode.md), and
      record the full opcode list in that spec.
  - Accept:
    - Every opcode encodes and decodes to itself, and the spec lists every opcode with its operands.
  - Verify: `cargo test -p lunaferra-bytecode --locked`

- [ ] **M2.2 — vm: heap of counted slots and generational handles**
  - Note: 2026-11-24, the owner's choice (ADR-0021): bytes count by fixed charges set in the memory
    spec, so they are the same on every target.
  - Depends: M0.1
  - Do:
    - Build the arenas, slots, handles, counting, and freeing work list of
      [`specs/memory.md`](specs/memory.md).
    - Count bytes by the fixed charges of
      [`specs/memory.md`](specs/memory.md#memory-accounting), with each buffer sized by Lunaferra's
      own growth rules, and record the charges this task sets there (ADR-0021).
  - Accept:
    - A stale handle returns the internal error.
    - Dropping a 10,000,000-node list doesn't overflow the stack.
    - A dropped heap has freed every byte.
    - Each charge is no smaller than `size_of` of the type it stands for, and a fixed sequence of
      allocations counts exactly the bytes its charges add up to.
  - Verify: `cargo test -p lunaferra-vm --locked`

- [ ] **M2.3 — vm: values, strings, and interning**
  - Depends: M2.2
  - Do:
    - Build `Value`, strings, interning of short strings, and the per-state hash seed, random
      unless the caller gives one (ADR-0020), as [`specs/vm.md`](specs/vm.md#strings) describes.
  - Accept:
    - Two short strings with the same bytes share one handle, and two states get different seeds.
    - Two states given the same seed hash every string alike.
  - Verify: `cargo test -p lunaferra-vm --locked`

- [ ] **M2.4 — vm: tables**
  - Note: 2026-11-24, the owner's choice (ADR-0020): probe steps count against the instruction
    limit, not a cap.
  - Note: 2026-11-24, the owner's choice (ADR-0023): the array part holds keys 0 to n − 1.
  - Depends: M2.3
  - Do:
    - Build tables with an array part for keys 0 to n − 1 and a hash part, a count of entries
      (ADR-0025), key normalization, `#`
      borders, `next`, reference keys hashed by handle, and each probe step past the first counted
      as a work unit, as [`specs/vm.md`](specs/vm.md#tables) describes.
  - Accept:
    - For each table in `tests/conformance/length.lua`, `#t` returns the border PUC-Rio Lua 5.5.1
      returns for its twin's table, whose integer keys are each one higher, once M2.14 can run it.
    - Two states given the same seed that build the same table, with strings, tables, and
      functions as keys, visit its keys in the same order.
    - Each table operation reports one work unit for each probe step past the first, for a set of
      keys crafted to collide under a known seed as for any other; M3.2 enforces the limit.
  - Verify: `cargo test -p lunaferra-vm --locked`

- [ ] **M2.5 — compiler: names, upvalues, globals, and self-recursion**
  - Note: 2026-11-24, the owner's choice (ADR-0025): names are local to the whole function that
    assigns them, unless `global` or `nonlocal` says otherwise.
  - Depends: M1.7, M2.1
  - Do:
    - Build name resolution by [`specs/compiler.md`](specs/compiler.md#name-resolution), with
      function-wide locals, `global` and `nonlocal` declarations, the check for a local read
      before assignment, upvalue capture, and the `SELF` rule of
      [`specs/compiler.md`](specs/compiler.md#self-recursion).
    - Add the scoping property test of [`specs/language.md`](specs/language.md#tests).
  - Accept:
    - A self-recursive local function has no upvalue for its own name; a reassigned one does, and
      so does `function f() ... end` at a chunk's top level when nothing else assigns `f`.
    - `if c then x = 1 else x = 2 end print(x)` reads the function's local; an assignment in a
      nested function makes its own local unless `nonlocal` names the outer one; only names under
      a `global` declaration compile to global writes; and `global *` makes them all global.
    - A `nonlocal` with no enclosing variable, and a `global` after the name's first use, are
      compile errors.
    - Assigning to a `<const>` local, a `for` variable, or a `global <const>` is a compile error
      with PUC-Rio's message.
  - Verify: `cargo test -p lunaferra-compiler --locked`

- [ ] **M2.19 — math: floating-point functions with the same bits on every target**
  - Note: added on 2026-11-24, when the owner chose math functions of Lunaferra's own (ADR-0027).
    It is listed where it is to be done, before M2.6, whose constant folding uses it.
  - Note: 2026-11-24, the owner accepted ADR-0028 (the targets, host services, math, and tests of
    the six platforms), which sets the crate and the accuracy.
  - Depends: M0.2
  - Do:
    - Build `lunaferra-math`, with the functions of §6.7 of the manual that IEEE 754 doesn't define
      exactly, such as sine, cosine, tangent, their inverses, `exp`, and `log` in every base, and
      float `**`, using only the operations
      [`03-platforms.md`](03-platforms.md#the-same-results-on-every-target) allows.
    - Store a file of inputs, each with its exact value to 40 digits and the bits of Lunaferra's
      result, including zeros, subnormal numbers, the largest floats, `inf`, `nan`, and the inputs
      near which each function's result changes fastest.
  - Accept:
    - Every stored result is within one unit in the last place of its exact value.
    - Every function gives the stored bits, which M3.18 then checks on every target.
  - Verify: `cargo test -p lunaferra-math --locked`

- [ ] **M2.6 — compiler: expressions, registers, and constant folding**
  - Depends: M2.5, M2.19
  - Do:
    - Compile every expression, including chained comparisons that evaluate each middle operand
      once, conditional expressions that evaluate one branch, truth tests, f-strings, and list and
      tuple literals (ADR-0025), allocate registers as
      [`specs/compiler.md`](specs/compiler.md#registers-and-expressions) describes, and fold
      constants.
  - Accept:
    - Folded and unfolded constant expressions give the same values, including at overflow,
      division by zero, and `**` of integers.
  - Verify: `cargo test -p lunaferra-compiler --locked`

- [ ] **M2.7 — compiler: statements and control flow**
  - Depends: M2.6
  - Do:
    - Compile every statement, including `goto`, both `for` forms, with the numeric `for`'s
      half-open range of [`specs/language.md`](specs/language.md#indexes-and-ranges-adr-0023), and
      `<close>` variables.
  - Accept:
    - Every prototype the compiler emits for the conformance scripts passes the verifier.
  - Verify: `cargo test -p lunaferra-compiler --locked`

- [ ] **M2.8 — vm: the interpreter loop, calls, and varargs**
  - Depends: M2.4, M2.7
  - Do:
    - Build the interpreter loop, frames, calls, returns, multiple results, varargs, named vararg
      tables numbered from 0, and proper tail calls.
  - Accept:
    - 10,000,000 tail calls in a loop use the stack of one.
  - Verify: `cargo test -p lunaferra-vm --locked`

- [ ] **M2.9 — vm: operators and number printing**
  - Note: 2026-11-24, the owner's choice (ADR-0023): operators give Python's results, `+` joins
    strings, and nothing converts between strings and numbers.
  - Note: 2026-11-24, the owner's choice (ADR-0025): truth follows Python's rule.
  - Note: 2026-11-24, the owner's choice (ADR-0027): float `**` comes from `lunaferra-math`, and
    every NaN prints as `nan` (ADR-0028).
  - Depends: M2.8, M2.19
  - Do:
    - Build the operators of [`specs/language.md`](specs/language.md#meanings) on numbers and
      strings, `in`, `is`, and chained comparisons, and number to string conversion with PUC-Rio's
      exact rules. Record each operator's edge cases and error messages, and the conversion rules,
      in [`specs/vm.md`](specs/vm.md#the-interpreter).
    - Add `tests/conformance/arithmetic.lua` and `tests/conformance/tostring.lua` for what Lua 5.5
      shares, and `tests/extensions/operators.lua` for the rest.
    - Build truth, with `__bool`, as [`specs/language.md`](specs/language.md#truth-adr-0025)
      defines it (ADR-0025), and add `tests/extensions/truth.lua`.
  - Accept:
    - Both conformance scripts print their recorded output, and `operators.lua` and `truth.lua`
      their written output.
    - `"10" + 1`, `1 / 0`, `1 << -1`, and `3.0 | 1` raise their recorded errors, and `2 ** 10` is
      the integer 1024.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

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
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M2.17 — vm: the deterministic loop collector**
  - Note: added on 2026-11-24, when the owner chose a deterministic loop collector on top of
    reference counting (ADR-0018). It is listed where it is to be done, after M2.10.
  - Depends: M2.10
  - Do:
    - Build candidate recording, trial deletion, the schedule by allocation count, the ephemeron
      rule, and the collection log, as [`specs/memory.md`](specs/memory.md#the-loop-collector)
      describes, plus the `collectgarbage` options in
      [`specs/memory.md`](specs/memory.md#collectgarbage).
    - Run each collection's finalizers in the call that ran it, as
      [`specs/memory.md`](specs/memory.md#finalizers) describes (ADR-0021).
    - Store each conformance script's collection log, counted bytes included, make the runner
      compare each run with it, and describe where the logs live and how they are recorded again in
      [`04-quality.md`](04-quality.md#conformance-scripts) (ADR-0021).
    - Add `tests/conformance/gc.lua`, which builds loops, drops them, calls `collectgarbage()`, and
      prints what weak tables still hold, and the ordering scripts in `tests/extensions/gc/`.
  - Accept:
    - 100,000 dropped tables that each refer to themselves are freed by one `collectgarbage()`, and
      the heap returns to its size before.
    - No object a register, the registry, or the host can reach is ever freed, in the property test
      of the memory spec.
    - Two runs of every conformance script with a fixed hash seed give the same collection log,
      which matches the log stored for the script.
    - `gc.lua` prints its recorded output.
  - Verify: `cargo test -p lunaferra-vm --locked`; `cargo test -p lunaferra --test conformance --locked`

- [ ] **M2.18 — vm: lists, tuples, and sets**
  - Note: added on 2026-11-24, when the owner chose lists, tuples, and sets beside tables
    (ADR-0025). It is listed where it is to be done, after M2.17.
  - Depends: M2.10, M2.17
  - Do:
    - Build lists, tuples, and sets as [`specs/vm.md`](specs/vm.md#lists-tuples-and-sets) and
      [`specs/language.md`](specs/language.md#lists-tuples-and-sets-adr-0025) describe: their
      arenas and charges in [`specs/memory.md`](specs/memory.md#memory-accounting), indexing,
      `#`, `in`, `==`, the orderings, `+`, `*`, the set operators, hashing tuples by value, the
      shared metatables, `type`'s names, the generic `for` over their values, and their place in
      the loop collector.
    - Add the tuple hashing property test of [`specs/language.md`](specs/language.md#tests), and
      `tests/extensions/collections.lua`.
  - Accept:
    - A list that holds itself, dropped, is freed by one `collectgarbage()`.
    - `[1, nil, 3]` has length 3, `xs[3]` and `xs[-4]` raise the index error, and a list used as a
      key raises the unhashable error.
    - Equal tuples built apart are one key in a table and one member in a set.
    - `collections.lua` prints its written output, once M2.14 can run it.
  - Verify: `cargo test -p lunaferra-vm --locked`

- [ ] **M2.11 — vm: errors and protected calls**
  - Depends: M2.10
  - Do:
    - Build errors as values, `<close>` on unwinding, and the variable descriptions in runtime
      messages, as [`specs/vm.md`](specs/vm.md#errors) describes.
    - Add `tests/conformance/errors.lua`, printing each message caught by `pcall`.
  - Accept:
    - Every message matches PUC-Rio's, including `(global 'x')`, `(local 'x')`, `(field 'x')`,
      `(upvalue 'x')`, and `(method 'x')`.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M2.20 — vm: spans and hints for runtime errors**
  - Note: added on 2026-11-24, when the owner asked for five improvements to draft (ADR-0029). It
    is listed where it is to be done, after M2.11.
  - Note: 2026-11-24, the owner accepted ADR-0029 (runtime errors with their source line, a caret,
    and a hint).
  - Depends: M2.11, M2.14
  - Do:
    - Have the compiler record each instruction's span in its debug information, and carry the span
      and the hints of ADR-0029 in `lunaferra::Error`, working out each hint only when the error
      reaches the host.
  - Accept:
    - Each hint has a test with a script that triggers it and a near miss that gets none.
    - Every message in `tests/conformance/errors.lua` is unchanged.
  - Verify: `cargo test -p lunaferra-vm -p lunaferra --locked`

- [ ] **M2.12 — vm: coroutines**
  - Depends: M2.11
  - Do:
    - Build threads, `resume`, `yield`, and `close` inside the VM, and the yield rules for native
      calls, as [`specs/vm.md`](specs/vm.md#coroutines) describes. The `coroutine` library comes in
      M3.8.
  - Accept:
    - VM tests resume and yield across `pcall`, and get PUC-Rio's error for a yield across
      `table.sort`.
  - Verify: `cargo test -p lunaferra-vm --locked`

- [ ] **M2.13 — stdlib: the base library**
  - Depends: M2.11
  - Do:
    - Build the base library of [`specs/stdlib.md`](specs/stdlib.md#libraries), with the argument
      checks shared by every library, which convert nothing between strings and numbers, and with
      `select` and `ipairs` counting from 0, as
      [`specs/stdlib.md`](specs/stdlib.md#arguments-and-errors) describes.
    - Assign object IDs as [`specs/memory.md`](specs/memory.md#object-ids) describes, and print
      them from `tostring` and `print` in the formats of
      [`specs/stdlib.md`](specs/stdlib.md#object-ids-in-output) (ADR-0019).
    - Add `tests/conformance/base.lua`, `tests/extensions/object_ids.lua`, and
      `tests/extensions/positions.lua` for `select` and `ipairs`.
  - Accept:
    - Every base function, and each of its argument errors, prints its recorded output.
    - `positions.lua` prints its written output, and a numeric string passed where a base function
      takes a number raises the wrong-type error.
    - `object_ids.lua` prints its written output, and no `tostring` result contains an address.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M2.14 — lunaferra: a minimal embedding API and the conformance runner**
  - Depends: M2.13, M2.17, M2.18
  - Note: 2026-11-24, the owner accepted ADR-0034 (handles with no lifetime, checked against their
    state), which sets the shape of every handle.
  - Note (from M0.5): `cargo xtask record` runs each twin with an empty standard input, so the
    runner should give each Lunaferra script an empty standard input too.
  - Do:
    - Build `Lua::new`, `Lua::load`, and `exec` and `eval` for `nil`, booleans, numbers, and strings, as
      [`specs/lunaferra.md`](specs/lunaferra.md) sketches.
    - Build the conformance runner in `crates/lunaferra/tests/conformance.rs`, as
      [`04-quality.md`](04-quality.md#conformance-scripts) describes, pairing each script with its
      twin's `.out` file.
    - Add the numeric `for` property test of [`specs/language.md`](specs/language.md#tests).
  - Accept:
    - Every script in `tests/conformance/` prints its recorded output, and every script in
      `tests/extensions/` its written output.
    - A script without a twin, or a twin without a script, fails the runner.
    - Generated numeric `for` loops run as Python's `range` does, including `for i = 3, 0, -1 do`,
      which visits 3, 2, and 1.
  - Verify: `cargo test -p lunaferra --test conformance --locked`;
    `cargo test -p lunaferra --locked`

- [ ] **M2.15 — cli: `lunaferra run`**
  - Depends: M2.14
  - Do:
    - Build `lunaferra run <script> [args...]`, with the `arg` table set as PUC-Rio's `lua` sets it.
  - Accept:
    - The conformance runner's binary pass prints every script's recorded output.
  - Verify: `cargo test -p lunaferra-cli --locked`

- [ ] **M2.21 — cli: runtime errors with their source line, a caret, and a hint**
  - Note: added on 2026-11-24 with M2.20 (ADR-0029).
  - Depends: M2.15, M2.20
  - Do:
    - Print runtime errors from `lunaferra run` in the text format of
      [`specs/diagnostics.md`](specs/diagnostics.md#output-formats).
  - Accept:
    - A script that calls `pirnt` prints its line with carets under `pirnt` and
      "did you mean `print`?", and exits with 1.
  - Verify: `cargo test -p lunaferra-cli --locked`

- [ ] **M2.16 — fuzz: compiler and interpreter targets**
  - Depends: M1.9, M2.14
  - Do:
    - Add the `compiler` and `interpreter` targets, which run generated programs under memory and
      instruction limits.
  - Accept:
    - One million runs of each from seed 0 find no crash.
  - Verify: `cargo xtask fuzz interpreter --seed 0 --runs 1000000` (expect no new saved input)

**Exit criteria (M2):** A person ports a few real Lua 5.5 scripts that use only the base library
to Lunaferra, runs them with `lunaferra run`, and their output matches what `lua` prints for the
originals.

---

## M3 — Embed Lunaferra with the full standard library and a sandbox

**Goal:** A Rust program embeds Lunaferra, registers its own functions and types, runs untrusted
scripts within limits, and gets every standard library it grants.

- [ ] **M3.1 — lunaferra: the full embedding API**
  - Depends: M2.14
  - Decide first: whether `Lua` implements `Send`, as [`specs/lunaferra.md`](specs/lunaferra.md#open-questions)
    describes.
  - Do:
    - Build values tied to their state, `IntoLua` and `FromLua`, with `Vec` and `HashSet`
      converting to lists and sets (ADR-0026), Rust functions, `UserData`, the
      registry, `Lua::collect`, the collector's options and `hash_seed`, a way to change the limits
      between calls (ADR-0021), and `Error`, then update the spec with the names the code settled.
  - Accept:
    - `Lua::collect` frees a loop the host dropped, and never a value the host still holds.
    - Using a value after its state is dropped returns `Error::StateDropped` (ADR-0034).
    - Using a value with another state returns an error.
  - Verify: `cargo test -p lunaferra --locked`

- [ ] **M3.21 — lunaferra: the host's API described once**
  - Note: added on 2026-11-24, when the owner asked for four embedding improvements to draft
    (ADR-0035). It is listed where it is to be done, after M3.1.
  - Note: 2026-11-24, the owner accepted ADR-0035 (the host's API described once).
  - Depends: M3.1
  - Do:
    - Build `Api`, the `TypeInfo` trait for every `FromLua` and `IntoLua` type, and
      `lua.install(&api)`, as [`specs/lunaferra.md`](specs/lunaferra.md#the-api) describes.
  - Accept:
    - An `Api` with a function, a module, and a userdata type installs bindings that scripts call,
      and `TypeInfo` gives the Lunaferra type of every type `FromLua` and `IntoLua` support.
  - Verify: `cargo test -p lunaferra --locked`

- [ ] **M3.2 — vm: limits**
  - Depends: M3.1
  - Decide first: the owner accepts ADR-0014 (the VM is the sandbox), and decides whether pending
    finalizers and `<close>` variables run with a reserve after a limit stops a script, and under
    what limit the finalizers that run when a state is dropped run.
  - Do:
    - Enforce every limit of [`specs/sandbox.md`](specs/sandbox.md#limits), including work units
      inside library calls, with hostile-input tests in `crates/lunaferra/tests/hostile_sandbox.rs`.
    - Count the instruction limit for each call from the host, finalizers included, and keep the
      finalizers a stopped collection didn't run for the next collection (ADR-0021).
    - Run the loop collector once before the memory limit stops a script.
  - Accept:
    - A script whose only garbage is loops runs within a memory limit it would pass without the
      collector.
    - Each limit stops a script one unit past it, inside `pcall`, `xpcall`, metamethods, and
      coroutines alike.
    - A set of keys crafted to collide under a known seed stops at the instruction limit
      (ADR-0020).
    - A script that runs just under the instruction limit runs in each of several calls in a row.
    - A `Lua::collect` whose finalizers pass its limit stops at the limit; the host's next call
      runs none of the rest, and the next `Lua::collect` runs them first, in order.
  - Verify: `cargo test -p lunaferra --test hostile_sandbox --locked`

- [ ] **M3.3 — stdlib: strings and patterns**
  - Depends: M2.13, M3.2
  - Do:
    - Build `string.byte` through `string.upper`, with the positions of
      [`specs/stdlib.md`](specs/stdlib.md#positions-and-ranges), and the pattern functions of
      §6.4.1, counting matcher steps.
    - Add `tests/conformance/string.lua` and `tests/conformance/patterns.lua`, extend
      `tests/extensions/positions.lua`, and add the `string.sub` property test of
      [`specs/language.md`](specs/language.md#tests).
  - Accept:
    - Both scripts print their recorded output, and a backtracking pattern stops at the
      instruction limit.
    - `string.sub` returns what Python's slices return in the property test.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M3.4 — stdlib: `string.format` and number formatting**
  - Depends: M3.3
  - Do:
    - Build float formatting and every `string.format` conversion, as
      [`specs/stdlib.md`](specs/stdlib.md#number-formatting) describes.
    - Make `%p` print object IDs (ADR-0019), and record what it prints for values with no
      address. Print every NaN as `nan` (ADR-0028).
    - Build f-string formatting, `format`, and `__format` with Python's mini-language, as
      [`specs/stdlib.md`](specs/stdlib.md#formatted-strings) describes (ADR-0025), and add
      `tests/extensions/format.lua` and the f-string property test of
      [`specs/language.md`](specs/language.md#tests).
    - Add `tests/conformance/string_format.lua`, with subnormal numbers, the largest and smallest
      floats, `inf`, and `nan`.
  - Accept:
    - The script prints its recorded output, and `%.17g` reads back every generated float.
    - `format.lua` prints its written output, and `f"{x:>{w}.{p}f}"` gives what `format` gives for
      the same spec.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M3.5 — stdlib: `string.pack`, `string.packsize`, and `string.unpack`**
  - Depends: M3.3
  - Do:
    - Build every format option of §6.4.2, with `string.unpack`'s positions counted from 0, and add
      `tests/conformance/pack.lua`.
  - Accept:
    - The script prints its recorded output, including each error for a malformed format.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M3.6 — stdlib: the `table` library**
  - Depends: M2.13
  - Do:
    - Build `table.create` and the rest of §6.6, with the positions of
      [`specs/stdlib.md`](specs/stdlib.md#positions-and-ranges) and `table.sort` following PUC-Rio's
      algorithm, and add `tests/conformance/table.lua`.
  - Accept:
    - The script prints its recorded output, including the order of equal elements after `sort`.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M3.15 — stdlib: the `list`, `tuple`, and `set` libraries**
  - Note: added on 2026-11-24, when the owner chose lists, tuples, and sets beside tables
    (ADR-0025). It is listed where it is to be done, after M3.6.
  - Depends: M2.18, M3.6
  - Do:
    - Build the libraries, the base functions `list`, `tuple`, and `set`, `string.join`, and
      `tostring` of a collection, as [`specs/stdlib.md`](specs/stdlib.md#lists-tuples-and-sets)
      describes, and record each settled signature there.
    - Extend `tests/extensions/collections.lua`.
  - Accept:
    - Each function has a test for each outcome and error, and `collections.lua` prints its written
      output.
    - `tostring` of a list that holds itself writes `...` at the repeat and stops.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M3.7 — stdlib: `math` and `utf8`**
  - Note: 2026-11-24, the owner's choice (ADR-0027): the math functions come from `lunaferra-math`.
  - Depends: M2.13, M2.19
  - Do:
    - Build §6.7 on `lunaferra-math`, with xoshiro256** seeded as PUC-Rio seeds it, and §6.5 with Lua
      5.5's `lax` argument, both with the ranges of
      [`specs/stdlib.md`](specs/stdlib.md#positions-and-ranges), and add
      `tests/conformance/math.lua` and `tests/conformance/utf8.lua`.
  - Accept:
    - `math.randomseed(42)` followed by ten `math.random(1, 101)` calls prints the numbers that
      PUC-Rio's `math.random(1, 100)` gives, for the same range of 1 to 100.
    - `math.lua` and its twin print float results with `%.12g`, so a last-bit difference from the
      C library that PUC-Rio uses doesn't show, and `lunaferra-math`'s stored bits check the rest.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M3.8 — stdlib: the `coroutine` library**
  - Depends: M2.12, M2.13
  - Do:
    - Build §6.2 on the VM's threads, and add `tests/conformance/coroutine.lua`.
  - Accept:
    - The script prints its recorded output, including `close` with pending `<close>` variables.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M3.22 — lunaferra: host functions that yield, and tasks**
  - Note: added on 2026-11-24, when the owner asked for four embedding improvements to draft
    (ADR-0037). It is listed where it is to be done, after M3.8.
  - Note: 2026-11-24, the owner accepted ADR-0037 (scripts that wait on the host).
  - Depends: M3.1, M3.2, M3.8
  - Do:
    - Build `Yield` for Rust functions, `Lua::spawn`, `Task::resume`, and `pause_on_limit`, as
      [`specs/vm.md`](specs/vm.md#coroutines) describes.
  - Accept:
    - A script that calls a host `wait()` three times yields three times, and the host resumes it
      to its end.
    - A task with `pause_on_limit` that loops past one call's budget pauses, goes on in the next
      call from the same instruction, and gives the same output as a run with no limit.
    - A `string.rep` that runs out partway stops the task with the limit error.
  - Verify: `cargo test -p lunaferra --locked`

- [ ] **M3.16 — lunaferra: a clock and a hash seed from the host**
  - Note: added on 2026-11-24, when the owner chose the web as a platform (ADR-0027). It is listed
    where it is to be done, before M3.9, whose time functions use it.
  - Depends: M3.1
  - Do:
    - Add `Options::clock`, and use it and `Options::hash_seed` where the standard library has no
      clock or source of randomness, as [`specs/lunaferra.md`](specs/lunaferra.md) describes
      (ADR-0028).
  - Accept:
    - On `wasm32-unknown-unknown`, `os.time` is absent without a clock and gives the host's time
      with one, and a state given no seed uses the fixed seed.
  - Verify: `cargo test -p lunaferra --locked`;
    `cargo build -p lunaferra --target wasm32-unknown-unknown --locked`

- [ ] **M3.9 — stdlib: `io` and `os`, under the file access policy**
  - Depends: M3.2, M3.16
  - Do:
    - Build §6.8 and §6.9, the file access policies of [`specs/sandbox.md`](specs/sandbox.md#file-access),
      and `os.exit` returning an exit error.
    - Show file handles by their IDs in `tostring` (ADR-0019).
  - Accept:
    - Every escape path in the sandbox spec's tests is refused under a directory policy.
  - Verify: `cargo test -p lunaferra --test hostile_sandbox --locked`

- [ ] **M3.10 — stdlib: `package` and `require`**
  - Depends: M3.9
  - Do:
    - Build `require` with the preload and Lua searchers through the host's loader, and
      `package.loadlib` returning an error (ADR-0017), and add `tests/conformance/require/`.
  - Accept:
    - Modules load once, circular `require` behaves as in PUC-Rio Lua 5.5.1, and `package.loadlib`
      returns `nil` and a message.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M3.23 — lunaferra: modules a host adds for `require`**
  - Note: added on 2026-11-24, when the owner asked for four embedding improvements to draft
    (ADR-0036). It is listed where it is to be done, after M3.10.
  - Note: 2026-11-24, the owner accepted ADR-0036 (modules a host adds for `require`).
  - Depends: M3.10
  - Do:
    - Build `Lua::add_module`, `Lua::add_source_module`, and `require` in every state, as
      [`specs/sandbox.md`](specs/sandbox.md#libraries) describes.
  - Accept:
    - In a state without `package`, `require("game")` returns the host's module, loads a source
      module once, and raises `module 'x' not found` with a hint for an unknown name.
    - With `package` granted, host modules are found before its searchers.
  - Verify: `cargo test -p lunaferra --locked`

- [ ] **M3.11 — stdlib: the `debug` library**
  - Depends: M2.12
  - Do:
    - Build §6.10 on Lunaferra's frames, with locals and upvalues numbered from 0 (ADR-0024), and
      add `tests/conformance/debug.lua`.
    - Make `debug.upvalueid` return the upvalue's ID as an integer (ADR-0019).
  - Accept:
    - `debug.traceback` and `debug.getinfo` print PUC-Rio's output for the script's twin.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M3.12 — bytecode: precompiled chunks and the verifier**
  - Depends: M2.14
  - Do:
    - Build the chunk format, `string.dump`, binary `load`, and the verifier of
      [`specs/bytecode.md`](specs/bytecode.md#verifier), plus the `chunk` fuzz target.
  - Accept:
    - Each verifier rule rejects a chunk that breaks only that rule, and one million mutated
      chunks from seed 0 crash nothing.
  - Verify: `cargo test -p lunaferra-bytecode --locked`;
    `cargo xtask fuzz chunk --seed 0 --runs 1000000`

- [ ] **M3.13 — lunaferra: the library allowlist**
  - Depends: M3.2, M3.10, M3.11, M3.12
  - Do:
    - Build library selection, `Libraries::SAFE`, and `Libraries::ALL`, and the escape-attempt
      tests of [`specs/sandbox.md`](specs/sandbox.md#tests).
  - Accept:
    - A `SAFE` state has exactly the functions the sandbox spec lists.
  - Verify: `cargo test -p lunaferra --test hostile_sandbox --locked`

- [ ] **M3.20 — lunaferra: finite limits by default**
  - Note: added on 2026-11-24, when the owner asked for five improvements to draft (ADR-0030). It
    is listed where it is to be done, after M3.13.
  - Note: 2026-11-24, the owner accepted ADR-0030 (finite limits by default).
  - Depends: M3.2, M3.13
  - Do:
    - Set the default limits of ADR-0030, add `Options::trusted()` and `Limits::none()`, and name
      the setting that raises a limit in each limit error, as
      [`specs/sandbox.md`](specs/sandbox.md#limits) describes.
  - Accept:
    - A state made with `Options::default()` stops `while true do end` at the instruction limit,
      and a table that keeps growing at the memory limit, each with an error that names its
      setting.
    - A state made with `Options::trusted()` runs a script that needs more than the defaults to its
      end.
  - Verify: `cargo test -p lunaferra --test hostile_sandbox --locked`

- [ ] **M3.14 — cli: options for `lunaferra run`**
  - Note: 2026-11-24, the owner accepted ADR-0030 (finite limits by default): `lunaferra run` uses
    `Options::trusted()`, and `--sandbox` the defaults.
  - Depends: M3.13
  - Do:
    - Add `-e`, `-l`, and `--sandbox`, as PUC-Rio's `lua` handles them, and `LUNAFERRA_INIT` where
      `lua` reads `LUA_INIT` (ADR-0024).
  - Accept:
    - `lunaferra run --sandbox` refuses `io.open`, and `-e` and `-l` behave as in `lua`.
  - Verify: `cargo test -p lunaferra-cli --locked`

- [ ] **M3.19 — cli: an interactive prompt**
  - Note: moved here from the backlog on 2026-11-24, when the owner asked for five improvements to
    draft (ADR-0033). The backlog row becomes one for line editing at the prompt.
  - Note: 2026-11-24, the owner accepted ADR-0033 (an interactive prompt in M3).
  - Depends: M3.14, M2.21
  - Do:
    - Build the prompt of
      [`specs/cli.md`](specs/cli.md#the-interactive-prompt-adr-0033).
  - Accept:
    - Typing `x = 1` and then `x + 1` prints `2`, and a line that ends inside a function body
      continues after `...`.
    - Piping a script into `lunaferra` runs it without a prompt, and piping the same two lines into
      `lunaferra -i` prints `2`.
  - Verify: `cargo test -p lunaferra-cli --locked`

- [ ] **M3.17 — xtask: test runners for Android, iOS, and the web**
  - Note: added on 2026-11-24, when the owner chose six platforms and a browser harness of
    Lunaferra's own (ADR-0027).
  - Depends: M2.14
  - Do:
    - Build `cargo xtask test --target <triple>` for every target, as
      [`03-platforms.md`](03-platforms.md#how-tests-run-on-each-target) describes: native runs,
      Android through `adb`, iOS through `xcrun simctl spawn`, and the web harness, with its test
      module, its page, a local HTTP server written with the standard library, and the headless
      browser.
    - Describe each target's setup in `CONTRIBUTING.md`: the Android NDK and SDK, Xcode, and a
      browser.
  - Accept:
    - On each kind of target, a script that fails on purpose fails the run, and one that passes
      passes.
    - The web harness reports every script's result, and fails when the browser reports nothing
      within its time limit.
  - Verify: `cargo test -p xtask --locked`; `cargo xtask test --target wasm32-unknown-unknown`

- [ ] **M3.18 — ci: the interpreter on every target**
  - Note: added on 2026-11-24, when the owner chose all six platforms by the first release
    (ADR-0027).
  - Note (from M0.3): `cargo xtask coverage` and its tests have run only on x86-64 Linux so far. It
    adds `EXE_SUFFIX` to the names of the LLVM tools, and `xtask/tests/coverage.rs` expects Windows to
    print Cargo's failure status as `exit code: 101`.
  - Note (from M0.5): `cargo xtask record` and `xtask/tests/record.rs` have run only on x86-64 Linux.
    On Windows, PUC-Rio Lua writes its standard output in text mode, so `print` ends each line with
    `\r\n`, and a `.out` file recorded there differs from one recorded on Linux. Git can also
    rewrite the line endings of a `.out` file on checkout when `core.autocrlf` is on. A
    `.gitattributes` line such as `*.out -text` keeps the bytes the conformance runner compares.
  - Depends: M3.17, M0.6
  - Do:
    - Add a CI job for each target in [`03-platforms.md`](03-platforms.md#targets) that runs
      `cargo xtask test --target` there, and `cargo xtask coverage` on Linux, Windows, and macOS,
      with every action pinned to a full commit hash.
    - Run the invariants of [`03-platforms.md`](03-platforms.md#invariants-tested-on-every-target)
      on each target, interpreted.
  - Accept:
    - Every job passes, and a pull request that breaks one target fails that target's job.
  - Verify: open a pull request and observe its checks on GitHub; the owner then makes each job
    required for `main`

**Exit criteria (M3):** A person embeds Lunaferra in a small Rust program, registers a function and
a userdata type, and sees a hostile script stopped by each limit; CI runs the interpreter's tests
on every target.

---

## M14 — Build projects from packages, with every dependency pinned

**Goal:** A user creates a package with `lunaferra new`, adds dependencies from a folder or a Git
repository with `lunaferra add`, and runs it with `lunaferra run`. `Lunaferra.lock` pins the version
and commit of every dependency, and a module can `require` only its own package, the packages its
manifest lists, and its host's modules, as [`specs/package.md`](specs/package.md) describes
(ADR-0038: packages and modules, as Cargo has; ADR-0039: dependencies from paths and Git, pinned in
a lock file). The milestone was added on 2026-11-25, when the owner asked for a module system
based on Cargo. It sits after M3, because a state's packages build on `require` (M3.10, M3.23) and
`lunaferra run` (M3.14), and before M6, whose analysis reads the manifest (M6.3).

- [ ] **M14.9 — toml: values, parser, document, and writer**
  - Note: added on 2026-11-25, when the owner chose TOML files used the way Cargo uses them
    (ADR-0038). It is listed where it is to be done, before M14.1, which reads the manifest with it.
  - Depends: M0.2
  - Decide first: the owner accepts ADR-0038 (packages and modules).
  - Do:
    - Add `lunaferra-toml` to the workspace, in the crate map's order, with no dependencies.
    - Build [`specs/toml.md`](specs/toml.md): values with their spans, the parser and its limits,
      the document that keeps comments and layout, the writer, and the `toml` fuzz target, with
      hostile-input tests in `crates/toml/tests/hostile_toml.rs`.
  - Accept:
    - Each valid and invalid test document, written for a rule of TOML 1.0.0, is accepted or
      rejected as that rule says, and each error gives its line and column.
    - Printing an unedited document gives back its bytes, and changing one value changes only its
      line.
    - Nesting one level past the limit is an error, and one million mutated documents from seed 0
      crash nothing.
  - Verify: `cargo test -p lunaferra-toml --locked`;
    `cargo xtask fuzz toml --seed 0 --runs 1000000`

- [ ] **M14.1 — package: the manifest and config files**
  - Note: 2026-11-25, the owner chose TOML files used the way Cargo uses them, so this task reads
    `Lunaferra.toml` and `config.toml` with `lunaferra-toml` (M14.9) instead of building a reader
    for data-only Lua files; it was "package: data-only files and the manifest".
  - Depends: M14.9, M1.6
  - Decide first: the owner accepts ADR-0038 (packages and modules).
  - Do:
    - Add `lunaferra-package` to the workspace, in the crate map's order, with `lunaferra-syntax`,
      for the diagnostic type, and `lunaferra-toml` as its dependencies.
    - Build `Manifest`, with workspace inheritance, and `Config`, with every key, rule, and limit
      of [`specs/package.md`](specs/package.md#the-manifest-lunaferratoml) and
      [`specs/package.md`](specs/package.md#configuration-configtoml), errors as `M` diagnostics
      ([`specs/diagnostics.md`](specs/diagnostics.md#codes)), the `manifest` fuzz target, and
      hostile-input tests in `crates/package/tests/hostile_manifest.rs`.
  - Accept:
    - An unknown key, such as `[dev_dependencies]`, gives its `M` diagnostic with a hint naming
      `[dev-dependencies]`, and a bare version string gives the hint to add `git` or `path`.
    - A member's `version.workspace = true` takes the root's version, and `workspace = true` on a
      key the root doesn't define is an error.
    - A manifest one past each limit is rejected with its exact message, and one million mutated
      manifests from seed 0 crash nothing.
  - Verify: `cargo test -p lunaferra-package --locked`;
    `cargo xtask fuzz manifest --seed 0 --runs 1000000`

- [ ] **M14.2 — package: versions and requirements**
  - Depends: M14.1
  - Do:
    - Build `Version` and `VersionReq` under SemVer 2.0.0, with the requirement forms and series of
      [`specs/package.md`](specs/package.md#versions-and-requirements), and the `version` fuzz
      target.
  - Accept:
    - The precedence examples of SemVer 2.0.0's section 11 sort in its order.
    - `"0.4"` matches 0.4.9 and not 0.5.0, and `"1.4"` matches no pre-release of 1.5.0 or 2.0.0.
    - On generated versions, parsing a version's text gives the same version.
  - Verify: `cargo test -p lunaferra-package --locked`

- [ ] **M14.3 — package: the resolver and the lock file**
  - Depends: M14.2
  - Decide first: the owner accepts ADR-0039 (dependencies from paths and Git, pinned in a lock
    file).
  - Do:
    - Build `resolve` over the `Registry` trait, as
      [`specs/package.md`](specs/package.md#resolution) describes, with its step limit, and read
      and write `Lunaferra.lock` in the shape of
      [`specs/package.md`](specs/package.md#the-lock-file-lunaferralock), plus the `lock` fuzz
      target.
  - Accept:
    - On generated graphs, every chosen version meets every requirement on its series, and
      resolving twice writes the same bytes.
    - A conflict names each requirement on the package and the manifest that made it.
    - A crafted graph stops at the step limit with its error.
    - With a lock file, an unchanged manifest gives an unchanged lock file.
  - Verify: `cargo test -p lunaferra-package --locked`

- [ ] **M14.4 — package: modules, exports, and the module graph**
  - Depends: M14.3
  - Do:
    - Build `ModuleGraph`: module names to files, each package's namespaces, exports, and
      workspaces, as [`specs/package.md`](specs/package.md#packages-and-modules) describes.
  - Accept:
    - From another package, `json.lexer` is refused as private, and an exported `json.stream`
      resolves.
    - A name that isn't a namespace gives a hint naming the manifest to change.
    - A package that holds both `src/a.lua` and `src/a/init.lua` is rejected.
  - Verify: `cargo test -p lunaferra-package --locked`

- [ ] **M14.5 — lunaferra: packages in a state**
  - Depends: M14.4, M3.23
  - Do:
    - Build `Packages`, its loader trait, `Lua::add_packages`, and `ChunkOptions::package`, and
      the resolver trait of `lunaferra-stdlib` that `require` asks, as
      [`specs/package.md`](specs/package.md#how-a-state-uses-the-graph) describes, with tests in
      `crates/lunaferra/tests/packages.rs`.
  - Accept:
    - Two series of one package load side by side, and each module loads once per state.
    - In a state with `Libraries::SAFE`, a module requires only its own package, its dependencies,
      and the host's modules, and writing `package.loaded` changes no package's module.
    - A dependency named after a host module is refused when the packages are installed.
  - Verify: `cargo test -p lunaferra --test packages --locked`

- [ ] **M14.6 — cli: `new`, `init`, `add`, `remove`, `tree`, and `run` in a package**
  - Depends: M14.5, M3.14
  - Do:
    - Build `lunaferra new`, `init`, `add --path`, `remove`, `fetch` for path dependencies, `tree`,
      `run` with no script inside a package, and `--locked`, as
      [`specs/cli.md`](specs/cli.md#commands) lists. `add` and `remove` edit the manifest through
      the document of `lunaferra-toml`, which keeps its comments and layout.
  - Accept:
    - `lunaferra new game`, then `lunaferra run` in `game/`, prints the template's greeting.
    - `lunaferra add --path ../util` keeps the manifest's comments and layout.
    - With `--locked`, a command that would change the lock file fails and names the package.
  - Verify: `cargo test -p lunaferra-cli --locked`

- [ ] **M14.7 — cli: Git dependencies and the package cache**
  - Depends: M14.6
  - Do:
    - Fetch through `git` and keep the cache, as
      [`specs/package.md`](specs/package.md#fetching-with-git) describes, and build `update` and
      `--offline`, with hostile-input tests in `crates/cli/tests/hostile_git.rs` on local
      repositories.
  - Accept:
    - A `version` requirement resolves to the newest matching tag, and the lock file records its
      full commit hash.
    - An `ext::` URL, a URL that starts with `-`, and a path dependency that leaves its repository
      are each refused before Git runs.
    - A commit with a symbolic link, a submodule, a `..` path, or two paths that differ only in
      case is refused, and nothing of it reaches the cache.
    - With `--offline` and an empty cache, `lunaferra fetch` fails with a message naming the
      package.
  - Verify: `cargo test -p lunaferra-cli --locked`

- [ ] **M14.8 — cli: `lunaferra vendor`, and packages bundled in a host**
  - Depends: M14.7, M3.17
  - Do:
    - Build `lunaferra vendor`, which prints the `[source]` lines for `.lunaferra/config.toml`,
      the `source.vendor` setting, and a `Packages` loader over sources a host compiles into its
      binary, as [`specs/package.md`](specs/package.md#fetching-with-git) describes.
  - Accept:
    - A project vendored on Linux, with `source.vendor` set, runs with an empty cache and no
      network.
    - The web harness runs a vendored package bundled in its test module, with no file system.
  - Verify: `cargo test -p lunaferra-cli --locked`;
    `cargo xtask test --target wasm32-unknown-unknown`

**Exit criteria (M14):** A person builds a package that depends on a second package of their own
by path and a third from a Git repository, runs it, and commits the lock file; a fresh clone runs
the same versions with `--locked`; a module that requires a package its manifest doesn't list fails
with a message that names the manifest.

---

## M4 — Write Lunaferra: compound assignment, types, classes, and strict mode

**Goal:** Scripts can use compound assignment, type annotations, classes, and strict mode, as
[`specs/language.md`](specs/language.md) defines.

- [ ] **M4.1 — syntax: compound assignment**
  - Note: 2026-11-24, the owner's choice (ADR-0023): compound assignment follows Python's, with
    `**=` for power and `^=` for XOR, and no `++` or `..=`. The superset test this task had is gone,
    and the precedence test of M1.5 takes its place.
  - Depends: M1.9
  - Do:
    - Lex and parse the tokens and statement of
      [`specs/language.md`](specs/language.md#compound-assignment-adr-0023).
  - Accept:
    - `x--=1` is `x` followed by a comment, `x **= 2` and `x ^= 1` are compound assignments, and
      `x++` is a syntax error.
  - Verify: `cargo test -p lunaferra-syntax --locked`

- [ ] **M4.2 — compiler: compound assignment**
  - Note: 2026-11-24, the owner's choice (ADR-0023): no `++`.
  - Depends: M4.1, M2.14
  - Do:
    - Compile compound assignment, evaluating the target's parts once, and add
      `tests/extensions/compound_assignment.lua`.
  - Accept:
    - `t[f()] += g()` calls `f` once and `g` once, in that order, through metamethods too, and
      `s += "!"` joins strings.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M4.9 — vm: grow strings and lists in place when only one place holds them**
  - Note: added on 2026-11-24, when the owner asked for five improvements to draft (ADR-0032). It
    is listed where it is to be done, after M4.2, which compiles `+=`.
  - Note: 2026-11-24, the owner accepted ADR-0032 (strings and lists grown in place).
  - Depends: M4.2, M2.18
  - Do:
    - Build the rule of ADR-0032 in the VM, and add a benchmark to `benches/lua/`, with its Lua 5.5
      twin, that builds a string from many parts with `s += part`.
  - Accept:
    - The bytes the VM copies to build a string from n parts grow linearly with n, counted for
      n = 10,000 and n = 100,000.
    - A string or list that another variable also holds is copied, and both variables keep their
      values.
  - Verify: `cargo test -p lunaferra-vm --locked`

- [ ] **M4.3 — syntax: type annotations**
  - Depends: M4.1
  - Do:
    - Parse the annotation grammar of [`specs/language.md`](specs/language.md#type-annotations-adr-0006),
      have the compiler drop annotations and record JIT hints, and add the erasure property test.
  - Accept:
    - Generated typed programs compile to the same bytecode with and without their annotations,
      apart from hints.
  - Verify: `cargo test -p lunaferra-syntax -p lunaferra-compiler --locked`

- [ ] **M4.4 — syntax: class declarations**
  - Depends: M4.3
  - Decide first: the owner accepts ADR-0007 (classes with a fixed field layout), and answers its
    open questions in [`specs/language.md`](specs/language.md#classes-adr-0007-proposed).
  - Do:
    - Parse class declarations, and record the answers in the language spec.
  - Accept:
    - `class "Point"` is still a call, and `class Point ... end` is a class.
  - Verify: `cargo test -p lunaferra-syntax --locked`

- [ ] **M4.5 — vm: classes and instances**
  - Depends: M4.4
  - Do:
    - Compile and run classes, instances with a fixed layout, inheritance, `super`, and metamethod
      methods, and add `tests/extensions/classes/`.
  - Accept:
    - Creating and dropping 100,000 instances of a subclass returns the heap to its size before.
  - Verify: `cargo test -p lunaferra -p lunaferra-vm --locked`

- [ ] **M4.6 — compiler: strict mode and undeclared-variable errors**
  - Note: 2026-11-24, the owner accepted ADR-0035 (the host's API described once): the host's
    globals are the names its installed `Api` defines, which settles how a host lists them.
  - Depends: M4.1
  - Decide first: the owner accepts ADR-0008 (strict mode with `--!strict` or a host option).
  - Do:
    - Build the directive, the host option, and undeclared-variable errors, as
      [`specs/language.md`](specs/language.md#undeclared-variable-errors) describes, recording
      PUC-Rio's message there.
  - Accept:
    - The example in the spec gives its error, and without its `--!strict` line the same file runs
      to its end.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M4.7 — vm: arity enforcement**
  - Depends: M4.6
  - Do:
    - Emit and run `ARITY` checks for strict functions, with optional parameters, as
      [`specs/language.md`](specs/language.md#arity-enforcement) describes, and add
      `tests/extensions/strict/`.
  - Accept:
    - Strict scripts that finish print the same output with strict mode off.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

- [ ] **M4.8 — fuzz: the extensions in the generator**
  - Depends: M4.5, M4.7
  - Do:
    - Extend the program generator with every extension, and run the existing targets on it.
  - Accept:
    - One million runs of the `parser` and `interpreter` targets on extension programs crash
      nothing.
  - Verify: `cargo xtask fuzz interpreter --seed 1 --runs 1000000`

**Exit criteria (M4):** A person writes a small program using each extension, runs it, and checks
its output against the spec.

---

## M5 — Run hot code as x86-64 machine code

**Goal:** Hot functions run as machine code from the baseline JIT, every script prints the same
output with the JIT on or forced, and the agreed budgets are checked.

- [ ] **M5.1 — xtask: benchmarks and the benchmark machine**
  - ⏳ needs PUC-Rio Lua 5.5.1: running the comparison
  - Depends: M3.13
  - Do:
    - Write the benchmark scripts in `benches/lua/`, each with a Lua 5.5 twin in
      `benches/lua/reference/`, and `cargo xtask bench`, as
      [`04-quality.md`](04-quality.md#xtask) describes.
    - Record the benchmark machine and the first measurements in a note on this task and in
      [`00-overview.md`](00-overview.md#budgets).
  - Accept:
    - The runner prints the median time and peak memory of each script on Lunaferra and on
      PUC-Rio, the share of time spent updating counts and recording candidates, and the longest
      loop collection, for ADR-0018's thresholds.
  - Verify: `cargo xtask bench`

- [ ] **M5.10 — vm: specialize instructions in place**
  - Note: added on 2026-11-24, when the owner asked for five improvements to draft (ADR-0031). It
    is listed where it is to be done, after M5.1, whose benchmarks measure it.
  - Note: 2026-11-24, the owner accepted ADR-0031 (instructions specialized in place).
  - Depends: M5.1
  - Do:
    - Build specialized forms of the instructions M5.1's benchmarks spend the most time in, with the
      rewriting and rewriting back of ADR-0031, the verifier's rule against specialized opcodes,
      and `string.dump` writing the generic forms.
  - Accept:
    - Every conformance script prints its recorded output and its stored collection log with
      specialization on and off.
    - A function dumped after it specialized loads in another state, and the verifier rejects a
      chunk that holds a specialized opcode.
    - The interpreted benchmarks run faster, by an amount a note on this task records.
  - Verify: `cargo test -p lunaferra --test conformance --locked`; `cargo xtask bench`

- [ ] **M5.2 — jit: the x86-64 encoder**
  - Depends: M0.1
  - Do:
    - Build `jit::asm`, as [`specs/jit.md`](specs/jit.md#the-encoders) describes.
  - Accept:
    - Each instruction form encodes to the bytes the Intel manual gives, with tests citing the page.
  - Verify: `cargo test -p lunaferra-jit --locked`

- [ ] **M5.3 — jit: executable memory**
  - Depends: M5.2
  - Decide first: the owner accepts ADR-0012 (a method JIT) and ADR-0013 (`unsafe` only in the
    JIT's memory and entry modules).
  - Do:
    - Build `jit::memory`, with the crate's own `[lints]` table, as ADR-0013 describes.
  - Accept:
    - A test that reads `/proc/self/maps` while code exists finds no mapping with both `w` and `x`.
  - Verify: `cargo test -p lunaferra-jit --locked`

- [ ] **M5.4 — jit: entry, exits, and the `Tier` trait**
  - Note: 2026-11-24, the owner's choice: `JitMode` and the conformance runner's forced pass moved
    here from M5.8, because M5.5 to M5.7 check their work in `JitMode::Forced`.
  - Note (from M0.3): `cargo xtask coverage` takes no options. It runs `cargo test --workspace` with
    each crate's default features, so covering `lunaferra` with the `jit` feature off, as well as
    on, needs an option for it, such as `-p lunaferra`.
  - Depends: M5.3, M2.14
  - Do:
    - Build `jit::entry`, the `Tier` trait in `lunaferra-vm`, and exits to the interpreter, with
      layout assertions for every `#[repr(C)]` type, as [`specs/vm.md`](specs/vm.md#the-compiled-tier)
      describes.
    - Add the `lunaferra` crate's `jit` feature, off by default, which plugs the tier in
      (ADR-0022). Lint, test, and cover `lunaferra` with the feature off (`-p lunaferra`) and on
      (the workspace run), in CI and in "Definition of done" in [`../CLAUDE.md`](../CLAUDE.md), and
      check that `cargo tree -p lunaferra --edges normal` lists no `lunaferra-jit`, in
      [`04-quality.md`](04-quality.md#automated-checks) as well.
    - Add `JitMode` to `Options` in both builds of `lunaferra`, refused without the feature, as
      [`specs/jit.md`](specs/jit.md#modes) describes, and the forced pass to the conformance runner,
      which runs when the feature is on.
  - Accept:
    - A compiled function that does nothing but exit continues correctly in the interpreter.
    - Without the feature, `cargo tree -p lunaferra --edges normal --prefix none` lists no
      `lunaferra-jit`, and `lunaferra`'s tests pass with the feature off and on.
    - Without the feature, `Lua::new` refuses `JitMode::On` and `JitMode::Forced` with an error that
      names it.
    - With the feature, the conformance runner's forced pass prints every script's recorded output.
  - Verify: `cargo test -p lunaferra-jit -p lunaferra-vm --locked`;
    `cargo test -p lunaferra --locked`;
    `cargo test -p lunaferra --features jit --locked`

- [ ] **M5.5 — jit: baseline tier for arithmetic, tables, and control flow**
  - Depends: M5.4
  - Do:
    - Emit templates with type guards for every instruction outside calls, as
      [`specs/jit.md`](specs/jit.md#baseline-tier) describes.
  - Accept:
    - Conformance scripts without calls print their output in `JitMode::Forced`.
  - Verify: `cargo test -p lunaferra --features jit --test conformance --locked`

- [ ] **M5.6 — jit: calls, returns, and on-stack replacement**
  - Depends: M5.5
  - Do:
    - Compile calls, returns, varargs, and tail calls, and enter hot loops through on-stack
      replacement.
  - Accept:
    - Every conformance script prints its output in `JitMode::Forced`.
  - Verify: `cargo test -p lunaferra --features jit --test conformance --locked`

- [ ] **M5.7 — jit: limits and debug hooks in compiled code**
  - Note: 2026-11-24, the owner accepted ADR-0037 (scripts that wait on the host): compiled code
    hands the last instructions before the limit to the interpreter, so a task pauses at the same
    instruction.
  - Depends: M5.6
  - Do:
    - Count instructions and check memory in compiled code, and run functions with breakpoints or
      hooks interpreted.
    - Let the loop collector run only at safe points in compiled code, with every count exact.
  - Accept:
    - The limit tests of the sandbox spec pass in `JitMode::Forced`.
  - Verify: `cargo test -p lunaferra --features jit --test hostile_sandbox --locked`

- [ ] **M5.8 — lunaferra: JIT modes and differential fuzzing**
  - Note: 2026-11-24, the owner's choice: `JitMode` and the conformance runner's forced pass moved
    to M5.4, because M5.5 to M5.7 check their work in `JitMode::Forced`.
  - Depends: M5.7
  - Do:
    - Turn the `jit` feature on in `lunaferra-cli` and the fuzzer (ADR-0022).
    - Add `--jit` to `lunaferra run`, and the `differential` fuzz target, which compares the output
      and collection logs of the interpreter and the JIT.
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

## M13 — Run hot code as machine code on every platform that allows it

**Goal:** The baseline JIT runs on Linux, Windows, macOS, and Android, on x86-64 and AArch64, and
every script prints the same output with the JIT off, on, or forced, on each (ADR-0027). The
milestone was added on 2026-11-24, when the owner chose all six platforms by the first release.

- [ ] **M13.1 — jit: the AArch64 encoder**
  - Depends: M5.2
  - Do:
    - Build the AArch64 half of `jit::asm`, as [`specs/jit.md`](specs/jit.md#the-encoders)
      describes, with each instruction form's bytes taken from Arm's Architecture Reference Manual.
  - Accept:
    - Each instruction form encodes to the manual's bytes, and labels fix up across the whole
      range of each branch form.
  - Verify: `cargo test -p lunaferra-jit --locked`

- [ ] **M13.2 — jit: executable memory on Windows, macOS, and Android, and on AArch64**
  - Depends: M5.3, M3.18
  - Do:
    - Build `jit::memory` for each operating system, and the instruction cache flush on AArch64, as
      ADR-0013 (proposed) lists, each `unsafe` block with its `// SAFETY:` comment and a test.
  - Accept:
    - On every JIT target, a buffer written and then run returns what it should, and a buffer
      rewritten after it ran runs the new code.
    - The W^X test of [`03-platforms.md`](03-platforms.md#invariants-tested-on-every-target)
      passes on each target.
  - Verify: `cargo xtask test --target <triple>` for each JIT target in
    [`03-platforms.md`](03-platforms.md#targets)

- [ ] **M13.3 — jit: the baseline tier on AArch64**
  - Depends: M13.1, M13.2, M5.7
  - Do:
    - Emit the baseline tier's templates, calls, returns, on-stack replacement, limits, and debug
      hooks for AArch64, with the procedure call standard and Apple's and Microsoft's variants of
      it.
  - Accept:
    - Every conformance script prints its recorded output in `JitMode::Forced` on every AArch64
      target, with the collection log stored for it.
  - Verify: `cargo xtask test --target aarch64-unknown-linux-gnu`, and the same on each other
    AArch64 JIT target

- [ ] **M13.4 — jit: Windows on x86-64**
  - Depends: M13.2, M5.7
  - Do:
    - Emit the Windows x64 calling convention, and register unwind tables for generated code.
  - Accept:
    - Every conformance script prints its recorded output in `JitMode::Forced` on
      `x86_64-pc-windows-msvc`.
    - A debugger walks a stack through generated code to the host's frames.
  - Verify: `cargo xtask test --target x86_64-pc-windows-msvc`; observe the stack in a debugger on
    Windows

- [ ] **M13.5 — lunaferra: JIT modes on every target**
  - Depends: M13.2, M5.4
  - Do:
    - Make `JitMode::On` the default where a backend exists, compile `lunaferra-jit` to nothing on
      iOS and the web, and fall back to the interpreter when the operating system refuses
      executable memory, as [`specs/jit.md`](specs/jit.md#modes) describes (ADR-0028).
  - Accept:
    - A hardened macOS host without the `allow-jit` entitlement runs in `JitMode::On`, interpreted,
      and its state reports that the JIT isn't running; `JitMode::Forced` returns the error.
    - On iOS and the web, `Lua::new` refuses `JitMode::On` with an error that names the target.
  - Verify: `cargo xtask test --target aarch64-apple-darwin`;
    `cargo xtask test --target aarch64-apple-ios-sim`

- [ ] **M13.6 — ci: the JIT on every JIT target**
  - Depends: M13.3, M13.4, M13.5, M5.8
  - Do:
    - Run the forced conformance pass and a differential fuzz run of 60 s on each JIT target in CI.
  - Accept:
    - Every job passes on every JIT target.
  - Verify: open a pull request and observe its checks on GitHub

**Exit criteria (M13):** A person runs the benchmarks with the JIT on and off on an AArch64 Mac, a
Windows PC, and an Android device, and sees each script print the same output as on Linux.

---

## M6 — Type-check scripts before they run

**Goal:** `lunaferra check` reports type errors in annotated Lunaferra code, using inline
annotations, LuaCATS comments, and declarations for the standard library.

- [ ] **M6.1 — analysis: names, scopes, and symbols**
  - Depends: M4.3
  - Do:
    - Build name resolution and symbols on the lossless tree, as
      [`specs/analysis.md`](specs/analysis.md#names-and-scopes) describes.
  - Accept:
    - On generated programs, every name resolves as the compiler resolves it.
  - Verify: `cargo test -p lunaferra-analysis --locked`

- [ ] **M6.2 — analysis: control flow and data flow**
  - Depends: M6.1
  - Do:
    - Build control flow graphs, reaching definitions, and liveness.
  - Accept:
    - Unit tests give the expected graph for each statement kind, including `goto` and `break`.
  - Verify: `cargo test -p lunaferra-analysis --locked`

- [ ] **M6.3 — analysis: projects, modules, and the manifest**
  - Note: 2026-11-25, ADR-0038 (packages and modules, proposed) moves the settings of
    `.lunaferra.lua` into the `[lints]` and `[tools]` tables of the package manifest,
    `Lunaferra.toml`, a TOML file as the owner chose, and links a package's files through its
    module graph. This task now reads the manifest through `lunaferra-package` and depends on
    M14.4; it was "analysis: projects, modules, and `.lunaferra.lua`".
  - Depends: M6.2, M14.4
  - Decide first: the owner accepts ADR-0038.
  - Do:
    - Read the manifest's `[lints]` and `[tools]` as
      [`specs/diagnostics.md`](specs/diagnostics.md#configuration-lints-and-tools-in-the-manifest)
      describes, and link files through `require` as
      [`specs/analysis.md`](specs/analysis.md#modules) describes.
  - Accept:
    - A manifest with an unknown key in `[tools]` gives its `M` diagnostic with its span.
    - In a workspace of two packages, `require` resolves each module's value across the packages,
      and a private module of the other package gives its diagnostic.
  - Verify: `cargo test -p lunaferra-analysis --locked`

- [ ] **M6.4 — types: representation and subtyping**
  - Depends: M6.1
  - Do:
    - Build the types of [`specs/types.md`](specs/types.md#types), with `list<T>`, `tuple<A, B>`,
      and `set<T>` (ADR-0025), and the subtype relation.
  - Accept:
    - Subtyping is reflexive and transitive on generated types, and recursive types terminate.
  - Verify: `cargo test -p lunaferra-types --locked`

- [ ] **M6.5 — types: inference, narrowing, and checking**
  - Depends: M6.2, M6.4
  - Do:
    - Build checking as [`specs/types.md`](specs/types.md#checking) describes, and register the
      first `T` codes in the diagnostics spec.
  - Accept:
    - Each `T` code has a file that triggers it and a near miss that doesn't.
  - Verify: `cargo test -p lunaferra-types --locked`

- [ ] **M6.6 — types: declaration files and the standard library**
  - Depends: M6.5
  - Do:
    - Settle the declaration syntax in [`specs/types.md`](specs/types.md#declarations), and write
      declarations for every Lua 5.5 library function.
  - Accept:
    - With `check-all` on, the conformance scripts give no `T` diagnostics beyond their marked
      lines.
  - Verify: `cargo test -p lunaferra-types --locked`

- [ ] **M6.12 — types: declarations from a host's API**
  - Note: added on 2026-11-24 with M3.21 (ADR-0035).
  - Depends: M6.6, M3.21
  - Do:
    - Build `api.declarations()`, which writes an installed `Api` as a declaration file in the
      syntax of [`specs/types.md`](specs/types.md#declarations).
  - Accept:
    - `lunaferra check` reports a wrong argument type in a script that calls a host function
      declared this way, and the language server completes the host's names.
  - Verify: `cargo test -p lunaferra -p lunaferra-types --locked`

- [ ] **M6.7 — types: LuaCATS annotations**
  - Depends: M6.5
  - Do:
    - Read the LuaCATS tags of [`specs/types.md`](specs/types.md#luacats-annotations).
  - Accept:
    - A file annotated only with LuaCATS gets the same diagnostics as its inline-annotated twin.
  - Verify: `cargo test -p lunaferra-types --locked`

- [ ] **M6.8 — types: classes and strict arity**
  - Depends: M4.5, M4.7, M6.5
  - Do:
    - Check class fields, methods, and inheritance, and argument counts in strict files.
  - Accept:
    - Reading an undeclared field of an instance and a wrong argument count in a strict file each
      give their `T` diagnostic.
  - Verify: `cargo test -p lunaferra-types --locked`

- [ ] **M6.9 — json: values, parser, and writer**
  - Depends: M0.2
  - Do:
    - Build [`specs/json.md`](specs/json.md) without message framing, plus the `json` fuzz target.
  - Accept:
    - Every valid and invalid test document is accepted or rejected as RFC 8259 says.
  - Verify: `cargo test -p lunaferra-json --locked`

- [ ] **M6.10 — cli: `lunaferra check`**
  - Depends: M6.6, M6.9
  - Do:
    - Build `lunaferra check` with text and JSON Lines output, and the diagnostic registry test of
      [`specs/diagnostics.md`](specs/diagnostics.md#tests).
  - Accept:
    - The exit status follows [`specs/cli.md`](specs/cli.md#exit-status), and JSON output parses.
  - Verify: `cargo test -p lunaferra-cli --locked`

- [ ] **M6.11 — fuzz: analysis and type checker targets**
  - Depends: M6.6, M4.8
  - Do:
    - Add the `analysis` and `types` targets.
  - Accept:
    - One million runs of each from seed 0 crash nothing.
  - Verify: `cargo xtask fuzz types --seed 0 --runs 1000000`

**Exit criteria (M6):** A person annotates a real script, sees `lunaferra check` find a planted type
error, and sees no diagnostics on unannotated files.

---

## M7 — Lint and format scripts

**Goal:** `lunaferra lint` finds likely bugs, slow code, and reference loops, and `lunaferra fmt`
formats files without changing what they do.

- [ ] **M7.1 — lint: the rule engine, configuration, and suppression**
  - Depends: M6.3, M6.5
  - Do:
    - Build rule registration, severities from the manifest's `[lints]` table, and suppression
      comments, as [`specs/diagnostics.md`](specs/diagnostics.md#suppression-comments) describes.
  - Accept:
    - Each comment form suppresses exactly the codes and lines it names.
  - Verify: `cargo test -p lunaferra-lint --locked`

- [ ] **M7.2 — lint: correctness, porting, and memory rules**
  - Note: 2026-11-24, the owner's choice (ADR-0023): the porting rules join this task, to catch
    Lua habits that Python's operators and 0-based indexes turn into bugs.
  - Depends: M7.1
  - Do:
    - Build the correctness, porting, and memory rules of [`specs/lint.md`](specs/lint.md#rules),
      registering each code.
  - Accept:
    - Each rule has a triggering file and a near miss, `Class.__index = Class` is reported, and so
      are `x ^ 2`, `for i = 1, #t do`, and a counter closure that assigns an outer local without
      `nonlocal`.
  - Verify: `cargo test -p lunaferra-lint --locked`

- [ ] **M7.3 — lint: performance and style rules**
  - Depends: M7.1
  - Do:
    - Build the performance and style rules, with fixes where they are certain.
  - Accept:
    - Every offered fix leaves the bytecode unchanged apart from the fixed construct.
  - Verify: `cargo test -p lunaferra-lint --locked`

- [ ] **M7.4 — fmt: layout and comments**
  - Depends: M1.7
  - Do:
    - Build the document layout and the printer of [`specs/fmt.md`](specs/fmt.md#layout).
  - Accept:
    - The meaning, idempotence, and comment property tests pass.
    - `[[1], [2]]` written as a list comes out as `[ [1], [2] ]`, and f-string text is unchanged.
  - Verify: `cargo test -p lunaferra-fmt --locked`

- [ ] **M7.5 — fmt: options and the check mode**
  - Depends: M7.4, M6.3
  - Do:
    - Read the options of [`specs/fmt.md`](specs/fmt.md#options), and report `F` diagnostics.
  - Accept:
    - Every expected-output case passes with each option at its limits.
  - Verify: `cargo test -p lunaferra-fmt --locked`

- [ ] **M7.6 — cli: `lunaferra lint` and `lunaferra fmt`**
  - Depends: M7.2, M7.3, M7.5
  - Do:
    - Build both commands, with `--fix` and `--check`.
  - Accept:
    - `lunaferra fmt --check` exits with 1 on an unformatted file and 0 after `lunaferra fmt`.
  - Verify: `cargo test -p lunaferra-cli --locked`

- [ ] **M7.7 — ci: lint and format the repository's Lunaferra scripts**
  - Note: On 2026-11-25 the owner asked for the references to `.claude/rules/lua.md`, which M0.1
    removed, to go, so this task adds the two commands to the checks in `04-quality.md` and
    `CLAUDE.md` instead.
  - Depends: M7.6, M0.6
  - Do:
    - Format the Lunaferra scripts in `tests/` and `benches/`, excluding the Lua 5.5 twins in
      `reference/` folders in the `[tools]` table of a `Lunaferra.toml` at the repository's root,
      run both commands on them in CI, and add both to the
      [automated checks](04-quality.md#automated-checks) and to "Definition of done" in
      [`../CLAUDE.md`](../CLAUDE.md).
  - Accept:
    - CI fails on a pull request that adds an unformatted script.
  - Verify: `lunaferra fmt --check tests benches`; `lunaferra lint tests benches`

**Exit criteria (M7):** A person runs both commands on a real project, and its formatted code
still passes its tests.

---

## M8 — Find security problems in Lua code

**Goal:** `lunaferra sast` follows untrusted data to dangerous functions across a project and
reports each flow, in text or as SARIF for code scanning.

- [ ] **M8.1 — sast: sources, sinks, and sanitizers**
  - Depends: M6.3, M6.5
  - Do:
    - Build the built-in lists of [`specs/sast.md`](specs/sast.md#sources-sinks-and-sanitizers) and
      the `[tools.sast]` settings in the manifest.
  - Accept:
    - A host-declared source and sink are honored like built-in ones.
  - Verify: `cargo test -p lunaferra-sast --locked`

- [ ] **M8.2 — sast: taint analysis across functions and files**
  - Depends: M8.1
  - Do:
    - Follow taint through assignments, calls, returns, table fields, and string operations.
  - Accept:
    - A flow through five functions in three files is reported with its whole path, and recursion
      terminates.
  - Verify: `cargo test -p lunaferra-sast --locked`

- [ ] **M8.3 — sast: the security rules**
  - Depends: M8.2
  - Do:
    - Build each rule of [`specs/sast.md`](specs/sast.md#rules), registering each code.
  - Accept:
    - Each rule reports its flow and stays quiet through a sanitizer.
  - Verify: `cargo test -p lunaferra-sast --locked`

- [ ] **M8.4 — sast: SARIF output**
  - Depends: M8.3, M6.9
  - Do:
    - Write SARIF 2.1.0 logs with rules, results, and code flows.
  - Accept:
    - The log has every field SARIF 2.1.0 requires, and GitHub accepts it in a test upload to a
      scratch repository.
  - Verify: `cargo test -p lunaferra-sast --locked`; upload a log with
    `github/codeql-action/upload-sarif` and observe the results under **Security**

- [ ] **M8.5 — fuzz: the SAST target**
  - Depends: M8.3, M6.11
  - Do:
    - Add the `sast` target.
  - Accept:
    - One million runs from seed 0 crash nothing.
  - Verify: `cargo xtask fuzz sast --seed 0 --runs 1000000`

- [ ] **M8.6 — cli: `lunaferra sast`**
  - Depends: M8.4
  - Do:
    - Build the command, with `--format sarif`.
  - Accept:
    - The exit status follows the CLI spec, and the SARIF output matches the library's.
  - Verify: `cargo test -p lunaferra-cli --locked`

**Exit criteria (M8):** A person runs `lunaferra sast` on a script with a planted command injection
and sees the flow reported, in the terminal and in GitHub's code scanning.

---

## M9 — Edit with auto complete and live diagnostics

**Goal:** An editor with an LSP client gets type-aware completion, diagnostics as you type, hover,
navigation, rename, and formatting from `lunaferra lsp`.

- [ ] **M9.1 — json: message framing and JSON-RPC**
  - Depends: M6.9
  - Do:
    - Build the framing and JSON-RPC layer of [`specs/json.md`](specs/json.md#message-framing).
  - Accept:
    - A header that lies about its length is rejected without reading past the limit.
  - Verify: `cargo test -p lunaferra-json --locked`

- [ ] **M9.2 — lsp: lifecycle, documents, and diagnostics**
  - Depends: M9.1, M7.2, M8.3
  - Do:
    - Build initialization, shutdown, document sync with incremental edits, and published
      diagnostics.
  - Accept:
    - An end-to-end test opens a file, edits it, and receives the right diagnostics after each edit.
  - Verify: `cargo test -p lunaferra-lsp --locked`

- [ ] **M9.3 — lsp: completion**
  - Depends: M9.2
  - Do:
    - Build completion as [`specs/lsp.md`](specs/lsp.md#features) describes, on unparsed code too.
  - Accept:
    - After `p.` where `p` is an instance, the items are exactly its fields and methods.
  - Verify: `cargo test -p lunaferra-lsp --locked`

- [ ] **M9.4 — lsp: hover, signature help, navigation, and symbols**
  - Depends: M9.2
  - Do:
    - Build hover, signature help, definition, references, and document symbols.
  - Accept:
    - Each request gives the expected result on the end-to-end test project.
  - Verify: `cargo test -p lunaferra-lsp --locked`

- [ ] **M9.5 — lsp: rename, formatting, and code actions**
  - Depends: M9.4, M7.5
  - Do:
    - Build rename across files, formatting, and code actions from diagnostic fixes.
  - Accept:
    - Rename changes exactly the references that find-references returns.
  - Verify: `cargo test -p lunaferra-lsp --locked`

- [ ] **M9.6 — lsp: the completion budget**
  - Depends: M9.3, M5.9
  - Do:
    - Make the completion benchmark a check that fails over the agreed budget.
  - Accept:
    - The check passes on the benchmark machine, and fails on a deliberately slowed build.
  - Verify: `cargo xtask bench --check`

- [ ] **M9.7 — cli: `lunaferra lsp` and editor setup**
  - Depends: M9.5
  - Do:
    - Build the command, and a guide in `docs/` to configuring Neovim, Helix, Emacs, and Zed.
  - Accept:
    - A person follows the guide in one editor and gets completion and diagnostics.
  - Verify: `cargo test -p lunaferra-cli --locked`; follow the guide in Neovim and observe
    completion after `p.`

**Exit criteria (M9):** A person edits a real project in an editor and gets completion and
diagnostics without noticeable delay.

---

## M10 — Debug scripts from an editor

**Goal:** An editor with a DAP client sets breakpoints, steps, and inspects variables in scripts
launched by `lunaferra dap` or running inside a host.

- [ ] **M10.1 — vm: the debugger interface**
  - Depends: M5.7
  - Do:
    - Build breakpoints, stepping, pausing, and frame inspection on the VM's hooks, as
      [`specs/vm.md`](specs/vm.md#debug-hooks) describes.
  - Accept:
    - A breakpoint stops before the line's first instruction every time it runs, with the JIT on.
  - Verify: `cargo test -p lunaferra-vm --locked`

- [ ] **M10.2 — dap: lifecycle, launch, and breakpoints**
  - Depends: M10.1, M9.1
  - Do:
    - Build the protocol's lifecycle, `launch`, and every breakpoint kind of
      [`specs/dap.md`](specs/dap.md#features).
  - Accept:
    - An end-to-end test launches a script, stops at a conditional breakpoint, and continues.
  - Verify: `cargo test -p lunaferra-dap --locked`

- [ ] **M10.3 — dap: stack, variables, and evaluation**
  - Depends: M10.2
  - Do:
    - Build stack traces, scopes, variables with expansion, `evaluate`, `setVariable`, and stopping
      on errors.
    - Show objects by the IDs `tostring` prints (ADR-0019).
  - Accept:
    - Variables shown for a frame are exactly the locals and upvalues in scope.
  - Verify: `cargo test -p lunaferra-dap --locked`

- [ ] **M10.4 — lunaferra: a debug server inside a host**
  - Depends: M10.3
  - Do:
    - Let a host serve the protocol over a loopback TCP port, off by default.
  - Accept:
    - An editor attaches to a running host, stops at a breakpoint, and the host resumes afterward.
  - Verify: `cargo test -p lunaferra --locked`

- [ ] **M10.5 — cli: `lunaferra dap` and editor setup**
  - Depends: M10.3
  - Do:
    - Build the command, and add debugging to the editor guide.
  - Accept:
    - A person follows the guide and debugs a script in one editor.
  - Verify: `cargo test -p lunaferra-cli --locked`; follow the guide in Neovim and step through a
    script

**Exit criteria (M10):** A person debugs a script inside a small host program from an editor.

---

## M11 — Faster machine code from types and profiles

**Goal:** The optimizing tier makes hot code faster, using what the baseline tier observed and the
hints from annotations, within the optimizing JIT budget.

- [ ] **M11.1 — jit: SSA form and type feedback**
  - Note: 2026-11-24, the owner's choice (ADR-0027): the optimizing tier emits code for both
    backends, x86-64 and AArch64.
  - Depends: M5.8, M13.6
  - Do:
    - Build the SSA form of [`specs/jit.md`](specs/jit.md#optimizing-tier) from bytecode and the
      types the baseline tier recorded.
  - Accept:
    - Every conformance script prints its output with the optimizing tier forced.
  - Verify: `cargo test -p lunaferra --test conformance --locked`

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
  - Verify: `cargo test -p lunaferra-jit --locked`

- [ ] **M11.4 — jit: hints from type annotations**
  - Depends: M11.2
  - Do:
    - Use the compiler's hints as first guesses, always behind a guard (ADR-0006).
  - Accept:
    - A wrong annotation gives the right result through an exit, never a wrong value.
  - Verify: `cargo test -p lunaferra-jit --locked`

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

**Goal:** Lunaferra's crates are on crates.io with documentation, and a user can follow a guide to
embed Lunaferra and use its language and tools.

- [ ] **M12.1 — docs: the release document**
  - Depends: M11.5, M13.6
  - Do:
    - Write `docs/05-release.md`: build profiles, versioning, the publishing order of the crates,
      and the release checklist, which includes running the Android and iOS tests on a device
      (ADR-0028).
  - Accept:
    - The checklist names every step from tagging to publishing, in order.
  - Verify: read the rendered document

- [ ] **M12.2 — lunaferra: API review, READMEs, and docs.rs settings**
  - Note (from M0.1): No crate sets `version` yet, so Cargo gives each one 0.0.0 and won't publish
    it. Once the crates have versions, `lunaferra-testkit`, `lunaferra-fuzz`, `lunaferra-webtest`,
    and `xtask`, which the crate map keeps off crates.io, each need `publish = false`.
  - Depends: M12.1
  - Do:
    - Review every public item of every published crate by hand, give each crate a README, and set
      its docs.rs metadata.
  - Accept:
    - `cargo publish --dry-run` succeeds for every crate, in publishing order.
  - Verify: `cargo publish --dry-run -p <crate>` for each crate in order

- [ ] **M12.3 — docs: user guide**
  - Note: 2026-11-25, the guide also covers packages: making one, adding dependencies, and running
    a package in a host (ADR-0038, ADR-0039, both proposed; M14).
  - Depends: M12.2
  - Do:
    - Write a guide to embedding Lunaferra on each platform, to its language and how it differs from
      Lua 5.5, and to its tools, with examples that run as doc tests or conformance scripts.
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

**Exit criteria (M12):** The owner publishes the crates, and a new user embeds Lunaferra by
following the guide, on any of the six platforms.

---

## Backlog (each needs a trigger or a decision)

| Item | Trigger / reason |
| :- | :- |
| A WebAssembly JIT, which compiles hot functions to WebAssembly modules in the browser; the web runs interpreted today (ADR-0027) | A web host measures its scripts too slow interpreted |
| `wasm32-wasip1`, the WebAssembly System Interface, as a target | A host asks to run Lunaferra under WASI |
| 32-bit targets, such as Android on ARMv7 | A host asks for one |
| Arrow-key history and line editing at the interactive prompt, which need the terminal's raw mode and so `unsafe` system calls (ADR-0033) | The standard library gains a way to set raw mode, or the owner allows `unsafe` in `lunaferra-cli` |
| The Lua C API and C modules; today hosts must be Rust programs (ADR-0017) | A C or C++ host asks to embed Lunaferra |
| Incremental loop collection, which spreads one collection over several safe points to shorten pauses | A host measures a collection pause above the agreed budget that turning off automatic runs and scheduling collections can't avoid |
| Spreading the freeing of one large structure over several safe points | A benchmark measures a pause over 1 ms from freeing one value |
| Deferred counting of register references, which changes when `__gc` runs and needs a new record | M5.1 shows count updates above ADR-0018's threshold after M11.2 |
| Running lua.org's official 5.5.1 test suite, ported to Lunaferra; it isn't part of the platform today | The owner adds it to the platform in a decision record |
| A tool that ports Lua 5.5 scripts to Lunaferra's operators, indexes, and ranges, and reports what it can't change safely (ADR-0023) | A user reports porting a Lua library by hand, or a host asks for one |
| Coverage-guided fuzzing; stable Rust can't read coverage counters while running | A stable rustc option exposes coverage counters in process |
| Branch coverage instead of regions | rustc stabilizes branch coverage |
| A mutation testing tool of Lunaferra's own; none runs today | A bug ships past a test that ran its code without asserting on the result |
| A Visual Studio Code extension (ADR-0015) | The owner decides to publish one, with its client either written here or approved as an exception |
| Sharing compiled prototypes between states | A host measures the cost of loading the same scripts into many states |
| Re-recording conformance output for a new Lua 5.5 release | lua.org releases Lua 5.5.2 |
| A registry that finds packages by name, kept as a Git repository that records each version's Git URL and commit (ADR-0039); today a dependency is named by its URL | Ten public Lunaferra packages exist, or a user asks to find packages by name |
| Optional dependencies that a package turns on by name, as Cargo's features do (ADR-0038) | A package author asks to make a dependency optional |
| Module names relative to the requiring module, such as `require(".lexer")`, as Rust's `crate::` and `super::` give (ADR-0038) | A user reports that renaming a package meant editing every `require` inside it |
| A test runner, `lunaferra test`, that runs each script in a package's `tests/` | A package author asks for one |
| A checksum over each package's files, to check path and vendored sources too (ADR-0039) | The standard library gains a cryptographic hash, or the owner approves one in a decision record |
| Downloads over HTTPS without Git (ADR-0039) | The standard library gains TLS, or a user can't install Git |
