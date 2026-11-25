# 03 — Platforms

Lunaferra runs on Linux, Windows, macOS, Android, iOS, and the web (ADR-0027: six platforms, a JIT
wherever one is allowed, and the same results everywhere). The interpreter is portable Rust, so
it builds for every target the standard library supports, and the same script gives the same
results on each. The JIT needs a backend for each processor and code for each operating system's
executable memory, so it runs on Linux, Windows, macOS, and Android, on x86-64 and AArch64. iOS
and the web run the interpreter only. Development happens on Linux on x86-64, and from M3.18 on,
CI builds and tests every target.

## Targets

ADR-0028 (the targets, host services, math, and tests) sets these targets.

| Platform | Rust targets | Interpreter in CI | JIT |
| :- | :- | :- | :- |
| Linux | `x86_64-unknown-linux-gnu` | M0.6 | x86-64, M5 |
| Linux | `aarch64-unknown-linux-gnu` | M3.18 | AArch64, M13 |
| Windows | `x86_64-pc-windows-msvc` | M3.18 | x86-64, M13 |
| Windows | `aarch64-pc-windows-msvc` | M3.18 | AArch64, M13 |
| macOS | `aarch64-apple-darwin` | M3.18 | AArch64, M13 |
| macOS | `x86_64-apple-darwin` | M3.18 | x86-64, M13 |
| Android | `aarch64-linux-android` | M3.18 | AArch64, M13 |
| Android | `x86_64-linux-android` | M3.18 | x86-64, M13 |
| iOS | `aarch64-apple-ios`, and `aarch64-apple-ios-sim` for the simulator | M3.18 | None |
| Web | `wasm32-unknown-unknown` | M3.18 | None |

- The library, `lunaferra`, targets all of them. The `lunaferra` command, the language server, and
  the debug adapter target Linux, Windows, and macOS.
- iOS has no JIT because an iOS app can't make memory executable without an entitlement Apple
  doesn't give ordinary apps. The web has none because a browser runs WebAssembly, not machine
  code. A WebAssembly JIT is in the roadmap's backlog.
- On iOS and the web, `lunaferra-jit` compiles to nothing, so a host can turn the `jit` feature on
  for every target. There the default mode is `JitMode::Off`, and `Lua::new` refuses the other
  modes with an error that names the target ([`specs/jit.md`](specs/jit.md#modes)).

## Why these choices

- **One backend per processor, not per platform.** x86-64 and AArch64 cover every target that
  allows a JIT. What differs between operating systems is how memory becomes executable and how
  functions are called, which is small next to an encoder and the baseline tier's templates.
- **The interpreter everywhere from M3 on.** Every target builds and tests the interpreter long
  before its JIT lands, so a platform bug shows up in the code that caused it, not at release.
- **The same results everywhere.** Hosts that replay recorded runs, or keep several machines in
  lockstep, need every target to compute the same values, collect at the same points, and print
  the same text. Integer arithmetic and IEEE 754's basic operations already agree everywhere, the
  heap counts bytes by fixed charges (ADR-0021), and Lunaferra formats numbers itself. The rest is
  below, under "The same results on every target".

## What each target needs

- **Linux on x86-64:** executable memory from `mmap` and `mprotect`; the System V AMD64 calling
  convention; a CI runner, `ubuntu-latest`.
- **Linux on AArch64:** an AArch64 encoder; the AArch64 procedure call standard; an instruction
  cache flush after writing code, because AArch64 doesn't keep its instruction and data caches in
  step, done with the cache maintenance instructions through inline assembly; an AArch64 runner.
- **Windows:** executable memory from `VirtualAlloc` and `VirtualProtect`; `FlushInstructionCache`;
  the Windows x64 calling convention, and Microsoft's variant of the AArch64 standard; unwind tables
  registered for generated code with `RtlAddFunctionTable`, so crash reports can walk through it;
  Microsoft's compiler tools and SDK, which the `msvc` targets link with; runners for both
  processors. The file access policy treats drive letters, backslashes, and `\\?\` prefixes as
  hostile input too ([`specs/sandbox.md`](specs/sandbox.md#file-access)).
- **macOS:** memory mapped with `MAP_JIT`; on AArch64, `pthread_jit_write_protect_np` to switch a
  thread between writing and running code, which is how macOS enforces W^X there;
  `sys_icache_invalidate`; Apple's variant of the AArch64 standard; the
  `com.apple.security.cs.allow-jit` entitlement for hosts built with the hardened runtime, without
  which the JIT stays off and the state runs interpreted; Xcode's tools; a macOS runner, and
  Rosetta 2, Apple's x86-64 translator, for the x86-64 build if GitHub offers no Intel runner.
- **Android:** the same memory calls and cache flush as Linux; Google's Android NDK, whose linker
  the Android targets need, and SDK, whose emulator and `adb` run the tests; a Linux runner that can
  host the emulator.
- **iOS:** Xcode's tools and the iOS SDK; the simulator, which `xcrun simctl spawn` runs the tests
  in; a macOS runner. No JIT.
- **Web:** a clock and a hash seed from the host, because the standard library has neither on
  `wasm32-unknown-unknown` (`Options::clock` and `Options::hash_seed`,
  [`specs/lunaferra.md`](specs/lunaferra.md)); `io` and file-based `require` only through a host's
  loader; the headless browser on the CI runner for tests. No JIT.

Each operating system's executable memory adds `unsafe` code to `jit::memory`, under ADR-0013
(proposed: only the JIT's memory and entry modules may use `unsafe`), which lists each call.

## How tests run on each target

`cargo xtask test --target <triple>` runs the tests on any target in the table (M3.17):

- **Linux, Windows, and macOS:** `cargo test`, natively on a runner of that platform and processor.
- **Android:** `xtask` links the test programs with the NDK's linker, copies them and the test
  scripts to the emulator with `adb push`, runs them with `adb shell`, and reads their exit status.
- **iOS:** `xtask` builds the test programs for `aarch64-apple-ios-sim` and runs each in the
  simulator with `xcrun simctl spawn`.
- **Web:** Rust's test harness can't run in a browser, so `xtask` builds a test module of
  Lunaferra's own that runs scripts through `lunaferra`'s API, serves it with a small page from a
  local HTTP server written with the standard library, and opens the page in the runner's headless
  browser. The page runs every conformance and extension script, the saved fuzz inputs, and the
  stored collection logs, and posts each result back to the server. Rust's unit tests run
  natively.

The owner also runs the Android and iOS tests on a device before each release (M12), because
emulators and simulators don't run every processor feature a device has.

Coverage is measured on Linux, Windows, and macOS, and the floor in `.claude/rules/testing.md`
holds on each, so code that only one operating system compiles is covered on that one.

## The same results on every target

- **Math.** `math.sin`, `math.exp`, `math.log`, float `**`, and the other functions IEEE 754
  doesn't define exactly come from `lunaferra-math`, which computes them with only the operations
  IEEE 754 does define exactly, never with fused multiply-add or a platform's math library
  (M2.19). Each result is within one unit in the last place of the exact value.
- **NaN.** Processors give a new NaN different signs, so every NaN prints as `nan`, as Python
  prints it. `string.pack` still writes a NaN's bits as the processor made them, the one place
  they can differ (ADR-0028).
- **Stacks.** Mobile threads and browsers give smaller stacks than desktop threads, so the
  hostile-input tests at the nesting limit also run on a thread with a 256 KB stack.
- **Precompiled chunks** are little-endian on every target, so a chunk written on one loads on
  another ([`specs/bytecode.md`](specs/bytecode.md)).

## Adding a target

1. The owner widens the platform in `.claude/rules/dependencies.md`, and a decision record in
   [`decisions.md`](decisions.md) says why.
2. Add the target to `xtask test` and to CI, and run every check on it.
3. Run the invariants below on it, interpreted first, then with a JIT backend if it has one.
4. Update the table above, and the **Environments** in `.claude/rules/roadmap-and-docs.md`.

## Invariants tested on every target

- Every conformance script prints its recorded output, interpreted and, on targets with a JIT,
  with the JIT forced on.
- No memory mapping is writable and executable at the same time for any thread. On Linux and
  Android, a test reads `/proc/self/maps` while generated code exists and finds no mapping with
  both `w` and `x`.
- Every limit in [`specs/sandbox.md`](specs/sandbox.md) stops a script that exceeds it.
- With a fixed hash seed, every conformance script gives the collection log stored for it,
  counted bytes included, interpreted and compiled, so every target collects at the same points
  (ADR-0021: bytes are counted by fixed charges, alike on every target;
  [`specs/memory.md`](specs/memory.md#determinism)).
- Every function of `lunaferra-math` gives, for each input in its stored file, the bits stored for
  it.
- Every saved fuzz input in `fuzz/regressions/` runs without a crash.
- `cargo tree --workspace --edges normal,build,dev --target all --prefix none` lists only
  Lunaferra's own crates.
