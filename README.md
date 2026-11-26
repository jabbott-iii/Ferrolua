# Lunaferra

Lunaferra is a virtual machine with a just-in-time (JIT) compiler for a scripting language derived
from Lua 5.5, written in Rust, for Rust programs that embed one. It is designed to keep memory safe
whatever a script does, to use no third-party code, and to free memory deterministically: by
reference counting, with a loop collector that runs at points the program fixes, so every run frees
the same values in the same order.

**Status:** design and the first test tools. The design documents in [`docs/`](docs/) are
written, and the Cargo workspace has a crate for each part, apart from the two that packages need,
which wait for their decision records. The parser's crate has its first five pieces: byte spans
that mark places in source text, with line maps that turn a byte offset into a line and column,
counting lines as PUC-Rio Lua 5.5.1 does; the lexer, which splits source text into tokens, keeps
the spaces and comments around each one, and reports a malformed token with the message PUC-Rio
Lua 5.5.1 gives; the lossless syntax tree, which keeps every byte of the source and is built
from the events a parser emits, with typed views that name the parts of each node; the parser,
which reads every statement and every expression, with Python's operators and their precedence,
goes on after each syntax error, and gives the first error with the message and line PUC-Rio
Lua 5.5.1 gives where the syntax is Lua's; and the diagnostic type that every tool reports, with a
stable code for each kind of syntax error. The other
crates hold only their documentation, except the test tools: the test kit, which has a seeded random generator and a property-test runner; the fuzzer,
which runs its targets on mutated inputs and saves each input that makes one fail; and `xtask`,
whose `cargo xtask coverage` measures which lines and regions of code the tests run, whose
`cargo xtask fuzz` runs the fuzzer, and whose `cargo xtask record` saves what PUC-Rio
Lua 5.5.1 prints for the Lua 5.5 twin of each conformance script. The fuzzer has only the two
targets that test it so far. The three conformance scripts, `hello.lua`, `lexer.lua`, and
`syntax_errors.lua`, have their twins' output recorded, and wait for the conformance runner that
M2.14 builds. The continuous
integration (CI) workflow runs every check on each pull request, and each of its jobs must pass
before a merge. The next task is M1.7 in the [roadmap](docs/roadmap.md), which builds the compact
tree that the compiler reads, from the same events as the lossless tree.

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
- **Packages, as Cargo has them:** a TOML manifest, `Lunaferra.toml`, will name each package, its
  version, the modules it exports, and its dependencies, from a folder or a Git repository; a lock
  file, `Lunaferra.lock`, will pin the version and commit of each; and a module will reach only
  its own package and the packages its manifest lists, as the [package spec](docs/specs/package.md)
  plans.
- **Tools in one binary:** `lunaferra run`, `check` (types), `lint`, `fmt`, and `sast` (security),
  the package commands `new`, `add`, `fetch`, `update`, `tree`, and `vendor`, plus a language
  server for editor auto complete and a debug adapter for editor debugging.

[`docs/00-overview.md`](docs/00-overview.md) lists the goals, budgets, and risks, and
[`docs/01-architecture.md`](docs/01-architecture.md) the crates and how they fit together.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) describes how to set up, check, and propose a change. Report
security vulnerabilities privately, as [`SECURITY.md`](SECURITY.md) describes.

## License

Lunaferra is released under the [MIT License](LICENSE).
