# 03 — Platforms

Ferrolua is developed on Linux on x86-64 first. The interpreter is portable Rust, so it builds
wherever the standard library does, but each JIT backend is written for one processor and one
operating system. Today the project's platform, as `.claude/rules/dependencies.md` defines it,
includes only Linux on x86-64. Adding a target is the owner's decision, and it widens that
definition.

## Targets

| Order | Target | Rust target | Interpreter | JIT |
| :- | :- | :- | :- | :- |
| 1 | Linux on x86-64 | `x86_64-unknown-linux-gnu` | M2 | M5 |
| 2 | Linux on AArch64 | `aarch64-unknown-linux-gnu` | Backlog | Backlog |
| 3 | macOS on AArch64 | `aarch64-apple-darwin` | Backlog | Backlog |
| 4 | Windows on x86-64 | `x86_64-pc-windows-msvc` | Backlog | Backlog |

The order follows where embedded scripting runs most, and how much each target shares with the
one before it:

- Linux on AArch64 shares the operating system with target 1, so it needs only a second encoder.
- macOS on AArch64 shares that encoder, but not the system calls.
- Windows needs a new calling convention and new system calls.

## What each target needs

- **Linux on x86-64:**
  - executable memory from `mmap` and `mprotect`;
  - the System V AMD64 calling convention;
  - a CI runner, `ubuntu-latest`.
- **Linux on AArch64:**
  - an AArch64 encoder;
  - an instruction cache flush after writing code, because AArch64 doesn't keep instruction and
    data caches in step;
  - the AArch64 procedure call standard;
  - an AArch64 CI runner.
- **macOS on AArch64:**
  - memory mapped with `MAP_JIT`;
  - `pthread_jit_write_protect_np` to switch a thread between writing and running code, which is
    how macOS enforces W^X;
  - the `com.apple.security.cs.allow-jit` entitlement for hosts built with the hardened runtime;
  - a macOS CI runner.
- **Windows on x86-64:**
  - executable memory from `VirtualAlloc` and `VirtualProtect`;
  - the Windows x64 calling convention;
  - unwind tables registered for generated code, so crash reports can walk through it;
  - a Windows CI runner.

Each new target also needs its own version of the unsafe modules, which ADR-0013 (only the JIT's
memory and entry modules may use `unsafe`) says to revisit for new system calls.

## Adding a target

1. The owner widens the platform in `.claude/rules/dependencies.md`, and a decision record in
   [`decisions.md`](decisions.md) says why.
2. Add a CI job that runs every check on that target.
3. Run the invariants below on that target, interpreted first, then with its JIT backend once it
   lands.
4. Update the table above, and the **Environments** in `.claude/rules/roadmap-and-docs.md`.

## Invariants tested on every target

- Every conformance script prints its recorded output, interpreted and with the JIT forced on, on
  targets that have a JIT.
- No memory mapping is writable and executable at the same time. On Linux, a test reads
  `/proc/self/maps` while generated code exists and finds no mapping with both `w` and `x`.
- Every limit in [`specs/sandbox.md`](specs/sandbox.md) stops a script that exceeds it.
- Every saved fuzz input in `fuzz/regressions/` runs without a crash.
- `cargo tree --workspace --edges normal,build,dev --target all --prefix none` lists only
  Ferrolua's own crates.
- Precompiled chunks are little-endian on every target, so a chunk written on one target loads on
  another ([`specs/bytecode.md`](specs/bytecode.md)).
