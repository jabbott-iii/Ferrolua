# 02 — Dependency Policy

Lunaferra uses no third-party code (ADR-0002: Lunaferra uses only the Rust toolchain, its
platforms, Git, GitHub, and PUC-Rio Lua 5.5 as a test reference; ADR-0027 widened the platforms to
six). This document explains the rule
for contributors, lists the exceptions, and records each build-or-buy decision. The rule itself, with
every detail an agent or reviewer applies, is [`.claude/rules/dependencies.md`](../.claude/rules/dependencies.md).

## The rule

Every dependency is code that someone else can change. A host that embeds Lunaferra runs every line
of it next to scripts it didn't write, so Lunaferra writes what it needs instead of adding a
package. These count as dependencies: a crate from crates.io or a Git URL, every crate that crate
brings in, dev-dependencies, build tools, test, lint, and format tools, copied or vendored code, a
CI action, and any file or service fetched while building or running.

This rule covers Lunaferra's own code. The packages that a project's scripts depend on are that
project's choice, and `lunaferra`'s package commands fetch them with Git, which is part of the
platform (ADR-0039, proposed: dependencies from paths and Git, pinned in a lock file).

These don't count, because they are the platform Lunaferra is built on:

- Rust's toolchain from rustup at the version in `rust-toolchain.toml`: rustc, Cargo, the standard
  library, rustdoc, rustfmt, Clippy, and the `llvm-tools` component;
- PUC-Rio Lua 5.5 from lua.org, the reference interpreter for the part of the language Lunaferra
  keeps from Lua 5.5, which records the expected output of the conformance scripts' Lua 5.5 twins
  and is never built into or linked with Lunaferra;
- the six platforms of [`03-platforms.md`](03-platforms.md) and their own tools (ADR-0027: six
  platforms): Linux on x86-64 and AArch64, with the C library that Rust's standard library
  already links; Windows, with Microsoft's compiler tools and Windows SDK; macOS and iOS, with
  Apple's Xcode, its SDKs, and the iOS simulator; Android, with Google's Android SDK, NDK, and
  emulator; and the web, through the browsers that run WebAssembly;
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
   Lunaferra needs, why each way to avoid the dependency fails, which reason above applies, and the
   evidence. It names the package, its version, its owners on the registry, its license, and the
   number of crates it brings in.
2. The owner accepts or rejects the record.
3. In the change that adds the dependency, list it under "Approved exceptions" below, with its
   version and the record. Commit `Cargo.lock` in the same change, pin a Git dependency or a CI
   action to a full commit hash, and use the package behind one module of Lunaferra's own.

An approval covers one package, for one reason, within one major version. For a version below 1.0,
such as 0.4.2, that means within 0.4.

## Approved exceptions

None.

## Check

This command must list only Lunaferra's own crates:

```bash
cargo tree --workspace --edges normal,build,dev --target all --prefix none
```

## Build-or-buy register

Each row is something Lunaferra needs, and how it gets it without a third-party package.

| Need | Decision | Where |
| :- | :- | :- |
| Expected output of Lua scripts | Record it with PUC-Rio Lua 5.5.1, installed by the contributor | `cargo xtask record` (M0.5) |
| Code coverage | rustc's `-C instrument-coverage`, with `llvm-profdata` and `llvm-cov` from `llvm-tools` | `cargo xtask coverage` (M0.3) |
| Fuzzing | A fuzzer of Lunaferra's own, generating and mutating inputs from a fixed seed | `lunaferra-fuzz` (M0.4) |
| Property tests and seeded random numbers in tests | A generator and test runner of Lunaferra's own | `lunaferra-testkit` (M0.2) |
| Benchmarks | Timing with `std::time::Instant` | `cargo xtask bench` (M5.1) |
| Lint and format checks for Rust | Clippy and rustfmt from the toolchain | Check commands in `.claude/rules/rust.md` |
| Lint and format checks for Lua | Lunaferra's own linter and formatter | `lunaferra-lint` and `lunaferra-fmt` (M7) |
| Catching breaking API changes | Comparing each release's public items by hand | Release checklist (M12.2) |
| Mutation testing | None yet; the backlog holds a tool of Lunaferra's own | — |
| Seed for string hashing | Randomness from the standard library's `RandomState` | `lunaferra-vm` (M2.3) |
| `math.random` | xoshiro256**, the generator PUC-Rio Lua 5.5 uses, so seeded sequences match | `lunaferra-stdlib` (M3.7) |
| Number printing and `string.format` | Float formatting of Lunaferra's own, matching PUC-Rio's output | `lunaferra-stdlib` (M3.4) |
| x86-64 and AArch64 machine code | Encoders of Lunaferra's own | `lunaferra-jit` (M5.2, M13.1) |
| Executable memory | Each operating system's own calls, declared against the libraries the standard library links | `lunaferra-jit` (M5.3, M13.2, ADR-0013) |
| Math functions with the same bits on every target | A math library of Lunaferra's own | `lunaferra-math` (M2.19, ADR-0027) |
| Tests on Android, iOS, and the web | The platforms' own `adb`, `xcrun simctl`, and browsers, driven by `xtask`, with a browser harness of Lunaferra's own | `cargo xtask test` (M3.17, ADR-0027) |
| JSON, JSON-RPC, and SARIF | A JSON crate of Lunaferra's own | `lunaferra-json` (M6.9) |
| Command-line arguments | A parser of Lunaferra's own, on `std::env::args_os` | `lunaferra-cli` (M1.10) |
| Tool configuration and package manifests | A data-only Lua file, read by Lunaferra's own parser | `Lunaferra.lua` (ADR-0016, ADR-0038) |
| Semantic Versioning, version resolution, and lock files | Code of Lunaferra's own | `lunaferra-package` (M14.2, M14.3) |
| Fetching packages, and checking that they are the code that was locked | The `git` command, which fetches over HTTPS and SSH and names every object by a hash of its contents | `lunaferra fetch` (M14.7, ADR-0039) |
| Editor integration | Lunaferra's own LSP and DAP servers; no editor extension yet | `lunaferra-lsp` and `lunaferra-dap` (ADR-0015) |
| Lua's official test suite | Not used: it isn't part of the platform as defined above | Backlog |
