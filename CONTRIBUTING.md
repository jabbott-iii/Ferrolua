# Contributing to Ferrolua

Ferrolua welcomes bug reports, fixes, tests, and documentation. For anything beyond a small fix,
open an issue first, so the design is agreed before you write the code.

Ferrolua is at its start. The repository holds its design in `docs/`, the rules its code, tests,
and documents follow in `.claude/rules/`, and a Cargo workspace whose crates, apart from the test
tools, hold only their documentation so far. [`docs/00-overview.md`](docs/00-overview.md) says what
Ferrolua is for, and [`docs/roadmap.md`](docs/roadmap.md) what comes next.

## Set up

1. Install Rust with [rustup](https://rustup.rs), then install the version Ferrolua uses, with the
   `llvm-tools` component that the coverage check needs:

   ```bash
   rustup toolchain install 1.100.0 --component llvm-tools
   ```

   rustup prints `1.100.0-x86_64-unknown-linux-gnu installed` on Linux on x86-64, or `unchanged`
   if you have it already. Inside the repository, Cargo then uses this version, because
   `rust-toolchain.toml` pins it.

2. To record the expected output of a conformance script, which comes from its Lua 5.5 twin, install
   PUC-Rio Lua 5.5.1, the reference interpreter, from [lua.org](https://www.lua.org/download.html)
   or your operating system's package manager. Then record the twin:

   ```bash
   cargo xtask record tests/conformance/reference/hello.lua
   ```

   It prints `tests/conformance/reference/hello.out: unchanged` when the `.out` file already holds
   what Lua printed, and `new` or `changed` when it saved the file. It runs `lua` from the `PATH`;
   set `FERROLUA_PUC_LUA` to the interpreter's path to run another one. With no script named, it
   records every twin.

There is nothing else to install. Ferrolua uses no third-party code, including test, lint, and
format tools, as [`dependencies.md`](.claude/rules/dependencies.md) describes.

## Make a change

- Put tests beside the code they cover, following the test layout in
  [`rust.md`](.claude/rules/rust.md) and [`lua.md`](.claude/rules/lua.md):
  - unit tests in a `#[cfg(test)] mod tests` beside the code
  - integration tests that use only the public API in each crate's `tests/`
  - Ferrolua conformance scripts in `tests/conformance/`, each with a Lua 5.5 twin in
    `tests/conformance/reference/` and a `.out` file beside the twin that holds the output PUC-Rio
    Lua 5.5.1 printed for it
  - scripts for behavior with no Lua 5.5 counterpart in `tests/extensions/`, each with a `.out`
    file written from the spec section it tests
  - fuzz targets in `fuzz/`, with every input that ever crashed one in `fuzz/regressions/`
- Write the tests in the same change as the code. Coverage stays at 100% of lines and regions, and
  a failing test means the code is wrong: fix the code, never the test.
  [`testing.md`](.claude/rules/testing.md) has the details.
- Use `unsafe` only where an accepted architecture decision record (ADR) allows it, as the unsafe
  policy in [`rust.md`](.claude/rules/rust.md) describes.
- Run every check before you open a pull request:

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --locked -- -D warnings
  cargo test --workspace --locked
  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
  cargo xtask coverage
  ```

  A passing run prints no warnings, and each command exits with status 0. `cargo test` ends the
  summary of each test binary with `test result: ok.` and `0 failed`, and `cargo xtask coverage`
  ends its report with `every line and region is covered`. When it doesn't, it lists each line and
  region that no test ran, as [`docs/04-quality.md`](docs/04-quality.md#xtask) describes.

## Commit

Commits follow [Conventional Commits](https://www.conventionalcommits.org/), as
[`commits.md`](.claude/rules/commits.md) describes:

```text
feat(syntax): parse goto statements and labels (M1.4)
```

- The scope is a crate's name without any `ferrolua-` prefix, such as `syntax` or `vm`, or
  `workspace`, `docs`, `ci`, or `claude`. Leave it out when a change spans several areas.
- End the summary line with the roadmap task's ID in parentheses when the commit finishes or
  advances one.
- Make each commit one logical change, and keep together the changes that belong together: a task's
  code, tests, and ticked checkbox; a change to how Ferrolua runs a script and the script that
  covers it, with its Lua 5.5 twin if it has one; and an `unsafe` block, its `// SAFETY:` comment,
  and its test.
- Mark a breaking change with a `BREAKING CHANGE:` trailer that names the decision record behind
  it.
- Write messages, comments, and documents as [`writing.md`](.claude/rules/writing.md) describes:
  American English, plain words, and code formatting for names, commands, and paths.

## Documentation

This section is the project's update map: it lists the documents each kind of change updates.
Update them in the same change, in the shape
[`roadmap-and-docs.md`](.claude/rules/roadmap-and-docs.md) gives.

| Change | Update |
| --- | --- |
| A change in behavior or design | The part's spec in `docs/specs/` |
| A decision made or changed | A decision record in `docs/decisions.md` |
| A roadmap task done | `docs/roadmap.md`, the status line in `README.md`, and the "Status" section of `CLAUDE.md` |
| A new command, module, or rule | The "Read before working" table in `CLAUDE.md` |
| A change to the workflow or the checks | This guide, the pull request template, and the issue forms in `.github/` |
| An approved exception to the dependency rule | `docs/02-dependency-policy.md` |
| A goal, budget, milestone, or task changed, once the owner agrees | `docs/00-overview.md` or `docs/roadmap.md` |

## Pull requests

- Open each pull request against `main`. Its template asks what changed and why, how you checked
  it, and what is left for later.
- Every check in "Make a change" must pass. CodeQL scans each pull request on GitHub; until a
  continuous integration (CI) workflow runs the other checks, run them yourself and list them in
  the description.
- [@jabbott-iii](https://github.com/jabbott-iii) reviews each pull request, as
  `.github/CODEOWNERS` assigns.

## Report a problem

- Report a bug or request a feature with the forms on the
  [new issue page](https://github.com/jabbott-iii/Ferrolua/issues/new/choose).
- Report a security vulnerability privately, as [`SECURITY.md`](SECURITY.md) describes, never in
  an issue.

## Terms

Ferrolua is released under the [MIT License](LICENSE). By contributing, you agree that your
contribution is released under the same license.

## Coding agents

Coding agents such as Claude Code follow the rules in `.claude/rules/` and can run the skills in
`.claude/skills/`. [`CLAUDE.md`](CLAUDE.md) lists the documents to read before working in each
area, and the checks a task must pass to be done.
