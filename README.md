# Ferrolua

Ferrolua is a Lua 5.5 virtual machine with a just-in-time (JIT) compiler, written in Rust, for Rust
programs that embed Lua. It is designed to keep memory safe whatever a script does, to use no
third-party code, and to free memory deterministically: by reference counting, with a loop
collector that runs at points the program fixes, so every run frees the same values in the same
order.

**Status:** design only. The design documents in [`docs/`](docs/) are written, and no code exists
yet. The next task is M0.1 in the [roadmap](docs/roadmap.md), which creates the Cargo workspace.

## What is planned

- **Lua 5.5:** every valid Lua 5.5 script will run as it does on PUC-Rio Lua 5.5.1, the reference
  interpreter, apart from the differences the [decision log](docs/decisions.md) records.
- **Memory safety:** the heap will be safe Rust, and `unsafe` code will be confined to the JIT's
  executable memory.
- **A sandbox:** hosts will choose which libraries each script gets, and the VM will enforce
  memory, instruction, and call-depth limits that a script can't catch. Objects will print as IDs
  such as `table: #42`, never as memory addresses.
- **Language extensions:** optional type annotations, classes, `++` and compound assignment such as
  `+=`, and a strict mode that reports wrong argument counts and undeclared variables.
- **Tools in one binary:** `ferrolua run`, `check` (types), `lint`, `fmt`, and `sast` (security),
  plus a language server for editor auto complete and a debug adapter for editor debugging.

[`docs/00-overview.md`](docs/00-overview.md) lists the goals, budgets, and risks, and
[`docs/01-architecture.md`](docs/01-architecture.md) the crates and how they fit together.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) describes how to set up, check, and propose a change. Report
security vulnerabilities privately, as [`SECURITY.md`](SECURITY.md) describes.

## License

Ferrolua is released under the [MIT License](LICENSE).
