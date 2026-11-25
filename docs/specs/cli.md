# Subsystem: `lunaferra-cli`

`lunaferra-cli` builds the `lunaferra` command: one binary that runs scripts, checks them, manages
packages, and starts the editor servers, for Linux, Windows, and macOS on x86-64 and AArch64
(ADR-0027: six platforms, where the library alone serves Android, iOS, and the web). It parses its
own arguments, with no third-party parser (ADR-0002). It is the only crate that reads environment
variables, writes to standard output and standard error, or starts another program: `git`, to fetch
packages (ADR-0039, proposed: dependencies from paths and Git, pinned in a lock file). The first
command, `parse`, lands in M1.10, and each later command lands with its milestone.

## Why one binary

Users install one program and learn one set of conventions: the same flags for paths, output
format, and configuration on every command, and the same exit codes. Each command is a thin layer
over a library crate, so the binary holds little logic of its own and its behavior is tested where
the logic lives.

## Commands

| Command | Does | Lands in |
| :- | :- | :- |
| `lunaferra parse <file>` | Reports syntax errors; `--tree` prints the lossless tree | M1.10 |
| `lunaferra run <script> [args...]` | Runs a Lunaferra script with every library, like PUC-Rio's `lua`, with `arg` set the same way; `-e`, `-l`, `--sandbox`, and `--jit`. With no script, inside a package, runs its `src/main.lua` | M2.15, M3.14, M14.6 |
| `lunaferra check [paths...]` | Type checks files and reports `P`, `C`, and `T` diagnostics | M6.10 |
| `lunaferra lint [paths...]` | Runs the lint rules; `--fix` applies certain fixes | M7.6 |
| `lunaferra fmt [paths...]` | Formats files in place; `--check` reports instead | M7.6 |
| `lunaferra sast [paths...]` | Runs the security rules | M8.6 |
| `lunaferra lsp` | Starts the language server on standard input and output | M9.7 |
| `lunaferra dap` | Starts the debug adapter on standard input and output | M10.5 |
| `lunaferra new <path>` | Creates a package in a new folder: a manifest, `src/init.lua`, and `src/main.lua` | M14.6 |
| `lunaferra init` | Makes the current folder a package, leaving its files as they are | M14.6 |
| `lunaferra add <name> --path <folder>` | Adds a dependency to the manifest, keeping its comments and layout; `--git <url>` with `--version`, `--tag`, `--branch`, or `--rev`, and `--dev` for a dev-dependency | M14.6, M14.7 |
| `lunaferra remove <name>` | Removes a dependency from the manifest | M14.6 |
| `lunaferra fetch` | Resolves the dependencies, writes `Lunaferra.lock` if a choice changed, and fetches every locked commit | M14.6, M14.7 |
| `lunaferra update [names...]` | Resolves again, ignoring the lock file's choices for the named packages, or for all | M14.7 |
| `lunaferra tree` | Prints the dependency graph, with each package's version and source | M14.6 |
| `lunaferra vendor` | Copies every locked package into `vendor/`, for builds with no network and for hosts that bundle their packages | M14.8 |
| `lunaferra` | With no arguments, starts an interactive prompt when standard input is a terminal, and otherwise runs standard input as a script (ADR-0033) | M3.19 |

The checking commands share `--format text|json|sarif` and `--manifest <file>`. With no paths, they
check the files the manifest's `tools.include` names; in a package without one, its `src/`,
`tests/`, and `examples/`; and otherwise every `.lua` file under the current directory.

The commands that read packages share `--locked`, which fails instead of changing
`Lunaferra.lock`, and `--offline`, which starts no network operation and uses only the cache
([`package.md`](package.md#the-lock-file-lunaferralock)).

## The interactive prompt (ADR-0033)

ADR-0033 (an interactive prompt in M3) gives learners a place to try code before they write a file.

- A line that parses as an expression prints its value, written as an f-string's `!r` writes it,
  as Python's prompt does. A chunk that isn't finished continues after a `...` prompt.
- Each line compiles as if `global *` covered its top level, into a table for the session that
  falls back to the real globals, so a name one line assigns is there on the next.
- Errors print with their source line, a caret, and a hint (ADR-0029).
- `lunaferra -i` starts the prompt whatever standard input is, as `lua -i` does, so tests can pipe
  lines into it.
- Line editing is the terminal's own: arrow-key history needs raw mode, which takes `unsafe`
  system calls, and the backlog holds it.
- The prompt and `lunaferra run` use `Options::trusted()`, and `--sandbox` the defaults (ADR-0030).

## Exit status

| Status | Meaning |
| :- | :- |
| 0 | Success, and no diagnostic at error severity |
| 1 | A diagnostic at error severity, a script that raised an error, or a package command that failed: a version conflict, a lock file that `--locked` kept from changing, or a fetch that Git couldn't finish |
| 2 | A usage error, or a file that couldn't be read |
| n | `lunaferra run` passes on the status from `os.exit(n)` |

## Environment

`lunaferra` reads `NO_COLOR`, which turns off colored output, and `LUNAFERRA_INIT` for
`lunaferra run`, which works as `LUA_INIT` does for PUC-Rio's `lua`: a chunk of Lunaferra code, or
`@` followed by a file name, run before the script. It doesn't read `LUA_INIT_5_5` or `LUA_INIT`,
which hold Lua 5.5 code (ADR-0024: the details Python's operators and 0-based indexes
leave open). The package commands read `LUNAFERRA_HOME`, the folder of the package cache, and
when it isn't set, `HOME`, or `USERPROFILE` on Windows, to find `.lunaferra` in the home folder.
They start `git` from the `PATH`. `lunaferra` reads no other environment variable.

## Invariants

1. Every command exits with a status from the table above.
2. No argument list makes the binary panic.
3. `lunaferra run` prints what PUC-Rio's `lua` prints for the script's Lua 5.5 twin and the same
   arguments.
4. At the interactive prompt, a name one line assigns has its value on every later line.
5. No package command runs code from a package: fetching, vendoring, and resolving read files
   only.
6. With `--locked`, no command changes `Lunaferra.lock`, and with `--offline`, no command starts a
   network operation.

## Tests

- End-to-end tests in `crates/cli/tests/` run the built binary with real arguments and files, and
  check its output and exit status.
- Invariant 2: a property test passes generated argument lists.
- Invariant 3: the conformance runner also runs each conformance script through the binary, and
  compares the output with its twin's `.out` file.
- Invariant 4: an end-to-end test pipes lines into `lunaferra -i`, assigning in one line and reading
  in the next (M3.19).
- Invariants 5 and 6: end-to-end tests run the package commands on local Git repositories that the
  tests create, as [`package.md`](package.md#tests) describes (M14.6, M14.7).
