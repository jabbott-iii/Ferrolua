# Lunaferra

Lunaferra is a virtual machine with a just-in-time (JIT) compiler for a language derived from Lua
5.5, with Python's operators and indexes that start at 0, written in Rust with no third-party code,
for Rust programs that embed a scripting language. It frees memory deterministically, by reference
counting and a loop collector that runs at points the program fixes, and adds optional types,
classes, compound assignment, and a strict mode. It also ships a type checker, a linter, a
formatter, a security scanner, a language server, and a debugger.
[`docs/00-overview.md`](docs/00-overview.md) gives the goals, and
[`docs/roadmap.md`](docs/roadmap.md) the plan.

The rules in `.claude/rules/` govern all work here: code, tests, dependencies, documents, prose, and
commits. Read the rule for each kind of file you touch.

## Status

Design and the first test tools: the documents in `docs/` exist, and the Cargo workspace has every
crate of the crate map in `docs/01-architecture.md` except `lunaferra-toml` and `lunaferra-package`,
which M14.9 and M14.1 add once their decision records are accepted. Each crate holds only its crate
docs, except `lunaferra-syntax`, which holds source IDs, spans, and line maps (M1.1), the lexer
(M1.2), and the parser's events, the lossless tree built from them, and typed views of its nodes
(M1.3); `lunaferra-testkit`, which holds the seeded random generator and the property-test runner
(M0.2); `lunaferra-fuzz`, the fuzzer, with only its two fixture targets (M0.4); and `xtask`, which
holds `cargo xtask coverage` (M0.3), `cargo xtask fuzz` (M0.4), and `cargo xtask record` (M0.5).
`tests/conformance/` holds two scripts, `hello.lua` and `lexer.lua`, and their Lua 5.5 twins with
the twins' recorded `.out` files. `.github/workflows/ci.yml` runs a CI job for each automated check
and a fuzz job on every pull request, and each job must pass before a merge to `main` (M0.6). The
next task is M1.4, which adds the parser, with the markers that emit its events and the
statements it reads.

## Read before working

| Area | Read first |
| :- | :- |
| Anything | [`docs/00-overview.md`](docs/00-overview.md), [`docs/01-architecture.md`](docs/01-architecture.md), and the task in [`docs/roadmap.md`](docs/roadmap.md) |
| A decision, or a change to one | [`docs/decisions.md`](docs/decisions.md) |
| Dependencies, `Cargo.toml`, workflows | [`docs/02-dependency-policy.md`](docs/02-dependency-policy.md) and `.claude/rules/dependencies.md` |
| A new target or platform-specific code | [`docs/03-platforms.md`](docs/03-platforms.md) |
| Tests, fuzzing, coverage, CI | [`docs/04-quality.md`](docs/04-quality.md) and `.claude/rules/testing.md` |
| The language: syntax, semantics, operators, indexes, and extensions | [`docs/specs/language.md`](docs/specs/language.md), which says where it differs from the [Lua 5.5 Reference Manual](https://www.lua.org/manual/5.5/) |
| Memory, reference counting, the loop collector, `__gc`, weak tables | [`docs/specs/memory.md`](docs/specs/memory.md) |
| Libraries a host grants, limits, untrusted input | [`docs/specs/sandbox.md`](docs/specs/sandbox.md) |
| Diagnostics, codes, and `[lints]` and `[tools]` in `Lunaferra.toml` | [`docs/specs/diagnostics.md`](docs/specs/diagnostics.md) |
| Packages, which will be called craters (ADR-0040), modules, `require` across packages, `Lunaferra.toml`, `Lunaferra.lock`, `config.toml`, versions, and fetching with Git | [`docs/specs/package.md`](docs/specs/package.md) |
| `crates/syntax` | [`docs/specs/syntax.md`](docs/specs/syntax.md) |
| `crates/bytecode` | [`docs/specs/bytecode.md`](docs/specs/bytecode.md) |
| `crates/math` | [`docs/specs/math.md`](docs/specs/math.md) and [`docs/03-platforms.md`](docs/03-platforms.md) |
| `crates/compiler` | [`docs/specs/compiler.md`](docs/specs/compiler.md) |
| `crates/vm` | [`docs/specs/vm.md`](docs/specs/vm.md) and [`docs/specs/memory.md`](docs/specs/memory.md) |
| `crates/jit`, or any `unsafe` | [`docs/specs/jit.md`](docs/specs/jit.md), and ADR-0013 in [`docs/decisions.md`](docs/decisions.md) |
| `crates/stdlib` | [`docs/specs/stdlib.md`](docs/specs/stdlib.md) |
| `crates/lunaferra` | [`docs/specs/lunaferra.md`](docs/specs/lunaferra.md) |
| `crates/analysis` | [`docs/specs/analysis.md`](docs/specs/analysis.md) |
| `crates/types` | [`docs/specs/types.md`](docs/specs/types.md) |
| `crates/lint` | [`docs/specs/lint.md`](docs/specs/lint.md) |
| `crates/fmt` | [`docs/specs/fmt.md`](docs/specs/fmt.md) |
| `crates/sast` | [`docs/specs/sast.md`](docs/specs/sast.md) |
| `crates/json` | [`docs/specs/json.md`](docs/specs/json.md) |
| `crates/toml` | [`docs/specs/toml.md`](docs/specs/toml.md) |
| `crates/package` | [`docs/specs/package.md`](docs/specs/package.md) |
| `crates/lsp` | [`docs/specs/lsp.md`](docs/specs/lsp.md) |
| `crates/dap` | [`docs/specs/dap.md`](docs/specs/dap.md) |
| `crates/cli` | [`docs/specs/cli.md`](docs/specs/cli.md) |
| `crates/testkit`, `fuzz/`, `xtask/` | [`docs/04-quality.md`](docs/04-quality.md#the-projects-own-test-tools) |
| Lunaferra scripts and their Lua 5.5 twins in `tests/` and `benches/` | [`docs/04-quality.md`](docs/04-quality.md#conformance-scripts) |
| Documents, prose, commits | `.claude/rules/roadmap-and-docs.md`, `.claude/rules/writing.md`, and `.claude/rules/commits.md` |

## Definition of done

A task is done when every step below passes.

1. The task's `Accept` results hold, and its `Verify` commands pass.
2. The check commands of `.claude/rules/rust.md` pass:

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --locked -- -D warnings
   cargo test --workspace --locked
   RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
   ```

3. `cargo xtask coverage` reports 100% of lines and regions, and the steps in "Before finishing" in
   `.claude/rules/testing.md` find no weakened test.
4. `cargo tree --workspace --edges normal,build,dev --target all --prefix none` lists only
   Lunaferra's own crates, and the steps in "Before finishing" in `.claude/rules/dependencies.md`
   pass.
5. Every document the change affects is updated in the same change, as "Documentation" in
   [`CONTRIBUTING.md`](CONTRIBUTING.md#documentation) lists, and the steps in "Before finishing" in
   `.claude/rules/roadmap-and-docs.md` pass.
6. The steps in "Before finishing" in `.claude/rules/writing.md` pass for every changed text.
7. The reply ends with a commit message for each commit, as `.claude/rules/commits.md` describes.
