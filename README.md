# Ferrolua

Ferrolua is a virtual machine with a just-in-time (JIT) compiler for a scripting language derived
from Lua 5.5, written in Rust, for Rust programs that embed one. It is designed to keep memory safe
whatever a script does, to use no third-party code, and to free memory deterministically: by
reference counting, with a loop collector that runs at points the program fixes, so every run frees
the same values in the same order.

**Status:** design and the first test tools. The design documents in [`docs/`](docs/) are
written, and the Cargo workspace has a crate for each part. Each holds only its documentation,
except the test tools: the test kit, which has a seeded random generator and a property-test
runner; the fuzzer, which runs its targets on mutated inputs and saves each input that makes one
fail; and `xtask`, whose `cargo xtask coverage` measures which lines and regions of code the tests
run, whose `cargo xtask fuzz` runs the fuzzer, and whose `cargo xtask record` saves what PUC-Rio
Lua 5.5.1 prints for the Lua 5.5 twin of each conformance script. The fuzzer has only the two
targets that test it so far. The one conformance script, `hello.lua`, has its twin's output
recorded, and waits for the conformance runner that M2.14 builds. The next task is M0.6 in the
[roadmap](docs/roadmap.md), which adds the continuous integration (CI) workflow.

## What is planned

- **A language derived from Lua 5.5:** Lua's statements, tables, and libraries, with Python's
  operators, indexes that start at 0, ranges that stop before their end, f-strings, locals by
  default, lists, tuples, and sets, and rule for which values are false, as the
  [language spec](docs/specs/language.md) defines them. Where the language keeps Lua 5.5's
  behavior, a script will print what PUC-Rio Lua 5.5.1, the reference interpreter, prints for the
  same code written in Lua, apart from the differences the [decision log](docs/decisions.md)
  records.
- **Memory safety:** the heap will be safe Rust, and `unsafe` code will be confined to the JIT's
  executable memory.
- **A sandbox:** hosts will choose which libraries each script gets, and the VM will enforce
  memory, instruction, and call-depth limits that a script can't catch. Objects will print as IDs
  such as `table: #42`, never as memory addresses.
- **Language extensions:** optional type annotations, classes, compound assignment such as `+=`, and
  a strict mode that reports wrong argument counts and undeclared variables.
- **Six platforms:** Linux, Windows, macOS, Android, iOS, and the web, with the JIT on every one
  that allows it and the same results on each, as [`docs/03-platforms.md`](docs/03-platforms.md)
  plans.
- **Tools in one binary:** `ferrolua run`, `check` (types), `lint`, `fmt`, and `sast` (security),
  plus a language server for editor auto complete and a debug adapter for editor debugging.

[`docs/00-overview.md`](docs/00-overview.md) lists the goals, budgets, and risks, and
[`docs/01-architecture.md`](docs/01-architecture.md) the crates and how they fit together.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) describes how to set up, check, and propose a change. Report
security vulnerabilities privately, as [`SECURITY.md`](SECURITY.md) describes.

## License

Ferrolua is released under the [MIT License](LICENSE).
