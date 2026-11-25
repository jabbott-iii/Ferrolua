# Subsystem: `ferrolua-cli`

`ferrolua-cli` builds the `ferrolua` command: one binary that runs scripts, checks them, and starts
the editor servers, for Linux, Windows, and macOS on x86-64 and AArch64 (ADR-0027: six platforms,
where the library alone serves Android, iOS, and the web). It parses its own arguments, with no
third-party parser (ADR-0002), and is the only crate that reads environment variables or writes to
standard output and standard error. The first command, `parse`, lands in M1.10, and each later
command lands with its milestone.

## Why one binary

Users install one program and learn one set of conventions: the same flags for paths, output
format, and configuration on every command, and the same exit codes. Each command is a thin layer
over a library crate, so the binary holds little logic of its own and its behavior is tested where
the logic lives.

## Commands

| Command | Does | Lands in |
| :- | :- | :- |
| `ferrolua parse <file>` | Reports syntax errors; `--tree` prints the lossless tree | M1.10 |
| `ferrolua run <script> [args...]` | Runs a Ferrolua script with every library, like PUC-Rio's `lua`, with `arg` set the same way; `-e`, `-l`, `--sandbox`, and `--jit` | M2.15, M3.14 |
| `ferrolua check [paths...]` | Type checks files and reports `P`, `C`, and `T` diagnostics | M6.10 |
| `ferrolua lint [paths...]` | Runs the lint rules; `--fix` applies certain fixes | M7.6 |
| `ferrolua fmt [paths...]` | Formats files in place; `--check` reports instead | M7.6 |
| `ferrolua sast [paths...]` | Runs the security rules | M8.6 |
| `ferrolua lsp` | Starts the language server on standard input and output | M9.7 |
| `ferrolua dap` | Starts the debug adapter on standard input and output | M10.5 |
| `ferrolua` | With no arguments, starts an interactive prompt when standard input is a terminal, and otherwise runs standard input as a script (ADR-0033, proposed) | M3.19 |

The checking commands share `--format text|json|sarif` and `--config <file>`. With no paths, they
check the files `.ferrolua.lua` includes, or every `.lua` file under the current directory.

## The interactive prompt (ADR-0033, proposed)

This section stays a proposal until the owner accepts ADR-0033.

- A line that parses as an expression prints its value, written as an f-string's `!r` writes it,
  as Python's prompt does. A chunk that isn't finished continues after a `...` prompt.
- Each line compiles as if `global *` covered its top level, into a table for the session that
  falls back to the real globals, so a name one line assigns is there on the next.
- Errors print with their source line, a caret, and a hint (ADR-0029, proposed).
- Line editing is the terminal's own: arrow-key history needs raw mode, which takes `unsafe`
  system calls, and the backlog holds it.
- The prompt and `ferrolua run` use `Options::trusted()`, and `--sandbox` the defaults (ADR-0030,
  proposed).

## Exit status

| Status | Meaning |
| :- | :- |
| 0 | Success, and no diagnostic at error severity |
| 1 | A diagnostic at error severity, or a script that raised an error |
| 2 | A usage error, or a file that couldn't be read |
| n | `ferrolua run` passes on the status from `os.exit(n)` |

## Environment

`ferrolua` reads `NO_COLOR`, which turns off colored output, and `FERROLUA_INIT` for
`ferrolua run`, which works as `LUA_INIT` does for PUC-Rio's `lua`: a chunk of Ferrolua code, or
`@` followed by a file name, run before the script. It doesn't read `LUA_INIT_5_5` or `LUA_INIT`,
which hold Lua 5.5 code (ADR-0024: the details Python's operators and 0-based indexes
leave open). It reads no other environment variable.

## Invariants

1. Every command exits with a status from the table above.
2. No argument list makes the binary panic.
3. `ferrolua run` prints what PUC-Rio's `lua` prints for the script's Lua 5.5 twin and the same
   arguments.

## Tests

- End-to-end tests in `crates/cli/tests/` run the built binary with real arguments and files, and
  check its output and exit status.
- Invariant 2: a property test passes generated argument lists.
- Invariant 3: the conformance runner also runs each conformance script through the binary, and
  compares the output with its twin's `.out` file.
