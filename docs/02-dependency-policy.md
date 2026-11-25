# 02 — Dependency Policy

Ferrolua uses no third-party code (ADR-0002: Ferrolua uses only the Rust toolchain, the Linux
platform, Git, GitHub, and PUC-Rio Lua 5.5 as a test reference). This document explains the rule
for contributors, lists the exceptions, and records each build-or-buy decision. The rule itself, with
every detail an agent or reviewer applies, is [`.claude/rules/dependencies.md`](../.claude/rules/dependencies.md).

## The rule

Every dependency is code that someone else can change. A host that embeds Ferrolua runs every line
of it next to scripts it didn't write, so Ferrolua writes what it needs instead of adding a
package. These count as dependencies: a crate from crates.io or a Git URL, every crate that crate
brings in, dev-dependencies, build tools, test, lint, and format tools, copied or vendored code, a
CI action, and any file or service fetched while building or running.

These don't count, because they are the platform Ferrolua is built on:

- Rust's toolchain from rustup at the version in `rust-toolchain.toml`: rustc, Cargo, the standard
  library, rustdoc, rustfmt, Clippy, and the `llvm-tools` component;
- PUC-Rio Lua 5.5 from lua.org, the reference interpreter, which records the expected output of
  conformance scripts and is never built into or linked with Ferrolua;
- Linux on x86-64, including the C library that Rust's standard library already links;
- Git, GitHub, GitHub Actions, and the actions that the `actions` and `github` organizations
  publish, each pinned to a full commit hash.

Crates that `jabbott-iii` publishes on crates.io from repositories under `github.com/jabbott-iii`
count as first party, but the crates they bring in still count.

## When an exception is possible

A third-party dependency is allowed only when it can't be avoided, for one of these reasons:

- the platform requires it to build, run, or ship;
- a format, protocol, device, or service has no public specification, so only its owner's library
  can use it;
- a law, a contract, or a certification names it;
- writing it here would be a security hole, as with cryptography the standard library lacks.

Saving time, popularity, small size, or good maintenance isn't a reason on its own.

## The process for an exception

1. Write a decision record in [`decisions.md`](decisions.md) with status `Proposed`. It says what
   Ferrolua needs, why each way to avoid the dependency fails, which reason above applies, and the
   evidence. It names the package, its version, its owners on the registry, its license, and the
   number of crates it brings in.
2. The owner accepts or rejects the record.
3. In the change that adds the dependency, list it under "Approved exceptions" below, with its
   version and the record. Commit `Cargo.lock` in the same change, pin a Git dependency or a CI
   action to a full commit hash, and use the package behind one module of Ferrolua's own.

An approval covers one package, for one reason, within one major version. For a version below 1.0,
such as 0.4.2, that means within 0.4.

## Approved exceptions

None.

## Check

Once the workspace exists, this command must list only Ferrolua's own crates:

```bash
cargo tree --workspace --edges normal,build,dev --target all --prefix none
```

## Build-or-buy register

Each row is something Ferrolua needs, and how it gets it without a third-party package.

| Need | Decision | Where |
| :- | :- | :- |
| Expected output of Lua scripts | Record it with PUC-Rio Lua 5.5.1, installed by the contributor | `cargo xtask record` (M0.5) |
| Code coverage | rustc's `-C instrument-coverage`, with `llvm-profdata` and `llvm-cov` from `llvm-tools` | `cargo xtask coverage` (M0.3) |
| Fuzzing | A fuzzer of Ferrolua's own, generating and mutating inputs from a fixed seed | `ferrolua-fuzz` (M0.4) |
| Property tests and seeded random numbers in tests | A generator and test runner of Ferrolua's own | `ferrolua-testkit` (M0.2) |
| Benchmarks | Timing with `std::time::Instant` | `cargo xtask bench` (M5.1) |
| Lint and format checks for Rust | Clippy and rustfmt from the toolchain | Check commands in `.claude/rules/rust.md` |
| Lint and format checks for Lua | Ferrolua's own linter and formatter | `ferrolua-lint` and `ferrolua-fmt` (M7) |
| Catching breaking API changes | Comparing each release's public items by hand | Release checklist (M12.2) |
| Mutation testing | None yet; the backlog holds a tool of Ferrolua's own | — |
| Seed for string hashing | Randomness from the standard library's `RandomState` | `ferrolua-vm` (M2.3) |
| `math.random` | xoshiro256**, the generator PUC-Rio Lua 5.5 uses, so seeded sequences match | `ferrolua-stdlib` (M3.7) |
| Number printing and `string.format` | Float formatting of Ferrolua's own, matching PUC-Rio's output | `ferrolua-stdlib` (M3.4) |
| x86-64 machine code | An encoder of Ferrolua's own | `ferrolua-jit` (M5.2) |
| Executable memory | Linux's `mmap` and `mprotect`, declared against the C library | `ferrolua-jit` (M5.3, ADR-0013) |
| JSON, JSON-RPC, and SARIF | A JSON crate of Ferrolua's own | `ferrolua-json` (M6.9) |
| Command-line arguments | A parser of Ferrolua's own, on `std::env::args_os` | `ferrolua-cli` (M1.10) |
| Tool configuration | A data-only Lua file, read by Ferrolua's own parser | `.ferrolua.lua` (ADR-0016) |
| Editor integration | Ferrolua's own LSP and DAP servers; no editor extension yet | `ferrolua-lsp` and `ferrolua-dap` (ADR-0015) |
| Lua's official test suite | Not used: it isn't part of the platform as defined above | Backlog |
