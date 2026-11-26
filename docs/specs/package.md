# Subsystem: `lunaferra-package`

`lunaferra-package` organizes Lunaferra code into packages, the way Cargo organizes Rust code. A
package is a folder with a manifest, `Lunaferra.toml`, that names it, gives its version, lists the
modules other packages may use, and lists the packages it depends on; its modules are the files
under its `src/` folder. A lock file, `Lunaferra.lock`, records the exact version and Git commit of
every dependency, so every run of a project uses the same code. Both are TOML files in the shapes
Cargo gives `Cargo.toml` and `Cargo.lock`. The crate reads manifests, lock files, and the
`config.toml` files of the package commands, compares versions, chooses the version of each
dependency, and builds the module graph that `require` follows, for the embedding API and the
analysis tools alike. It does no input or output of its own: `lunaferra-cli` fetches dependencies
with Git, and a host reads files through its own loader. ADR-0038 (packages and modules, as Cargo
has) and ADR-0039 (dependencies from paths and Git, pinned in a lock file), both proposed, shaped
it. It lands in M14.

A package will be called a crater (ADR-0040: a Lunaferra package is named a crater, as Rust users
call a package a crate). This spec says "package" until the rename, and M14.1 settles which names
in the manifest, the commands, and the messages change with it.

## Why packages modeled on Cargo

Lua finds a module by trying the patterns in `package.path`, such as `./?.lua`, so any file can
`require` any other file it can reach, every module name shares one global namespace, and nothing
records which version of a library a program was tested with. That works for one program in one
folder, but not for a program built from libraries that other people write and change.

Cargo solves the same problem for Rust, and Rust programmers, who write every Lunaferra host,
already know its shape. Each package says in one file what it is and what it uses; a crate reaches
only the crates its manifest names; a module is private unless it is marked public; and a lock
file pins every version. Lunaferra copies that shape, its file formats included, and leaves out
what doesn't fit a scripting language or its security rules:

| Cargo | Lunaferra |
| :- | :- |
| `Cargo.toml` | `Lunaferra.toml`, with Cargo's section names and kebab-case keys |
| `Cargo.lock` | `Lunaferra.lock`, in `Cargo.lock`'s shape |
| `.cargo/config.toml`, and `config.toml` in `~/.cargo` | `.lunaferra/config.toml`, and `config.toml` in `~/.lunaferra` |
| A library crate, rooted at `src/lib.rs` | A package's root module, `src/init.lua` |
| A binary crate, `src/main.rs` | The package's program, `src/main.lua` |
| `mod a;`, in `src/a.rs` or `src/a/mod.rs` | Module `a`, in `src/a.lua` or `src/a/init.lua` |
| `pub mod a;` | `exports = ["a"]` in `[package]` |
| `use serde::de;` | `local de = require("serde.de")` |
| `[dependencies]` and `[dev-dependencies]` | The same tables |
| `[workspace]`, with `[workspace.package]`, `[workspace.dependencies]`, and `[workspace.lints]` | The same tables, plus `[workspace.tools]`, inherited with `workspace = true` |
| `[lints]` | `[lints]`, by diagnostic code, such as `L0001 = "off"` |
| Sources: crates.io, Git, and paths | Sources: Git and paths; a registry kept in Git is in the backlog |
| `CARGO_HOME`, by default `~/.cargo` | `LUNAFERRA_HOME`, by default `~/.lunaferra` |
| `cargo new`, `add`, `remove`, `fetch`, `update`, `tree`, and `vendor` | `lunaferra new`, `add`, `remove`, `fetch`, `update`, `tree`, and `vendor` |
| Build scripts, `build.rs` | None: building or installing a package runs none of its code |
| Features | None yet; the backlog holds optional dependencies |

The cost is a second layout to learn for Lua programmers used to `package.path`, a TOML parser of
Lunaferra's own ([`toml.md`](toml.md)), and file formats that become a public interface: a key,
once released, keeps its meaning. Scripts outside any package still run as they do today.

## Packages and modules

A package is a folder that holds a manifest with a `[package]` table. Its layout:

```text
game/
  Lunaferra.toml      the manifest
  Lunaferra.lock      the lock file, at the root of a package or a workspace
  src/
    init.lua          the root module: require("game")
    rules.lua         module game.rules
    ai/
      init.lua        module game.ai
      planner.lua     module game.ai.planner
    main.lua          the program that lunaferra run runs, never a module
  tests/              scripts that use the package as a dependent would
  examples/           the same, for examples
```

- A module is one file. The module name `a.b` names `src/a/b.lua`, or `src/a/b/init.lua` when
  that file doesn't exist. A package that holds both is rejected when it's loaded, so a name never
  means two files.
- Each part of a module name is a Lua name: a letter or `_`, then letters, digits, or `_`. File
  names that aren't, such as `my-module.lua`, aren't modules.
- `src/init.lua` is the root module, which `require("game")` returns. A package without one has no
  root module, and only its exported modules can be required.
- `src/main.lua` is the package's program. `lunaferra run` with no script runs it. No module can
  require it: like a Rust binary crate, it is a program to run, not a library to use.
- Every module of a package can require every other module of the same package. Another package
  can require the root module and the modules in `exports`, and no other: a module is private to
  its package unless the manifest exports it, as a Rust module is private unless it is `pub`.
  Exporting `ai` exports `game.ai` alone, not `game.ai.planner`.
- Files under `tests/` and `examples/` see the package the way a dependent does, through its root
  module and its exports, plus the package's dependencies and dev-dependencies. That checks the
  package's public interface, as Cargo's integration tests do.

## Names in `require`

Inside a package, the first part of a name in `require` is a namespace: a top-level name that the
requiring package can see. A package's namespaces are its own name, the names of its
dependencies (and, for `tests/` and `examples/`, its dev-dependencies), and the modules its host
added (ADR-0036: modules a host adds for `require`). The rest of the name picks a module inside
that namespace.

```lua
local game = require("game")          -- this package's root module
local planner = require("game.ai.planner")  -- one of its private modules
local json = require("json")          -- a dependency's root module
local stream = require("json.stream") -- a module that `json` exports
local host = require("host")          -- a module the host added
```

- Each package has namespaces of its own. Two packages can depend on different versions of
  `json`, and each one's `require("json")` returns the version it asked for. That is how Cargo
  links two incompatible versions of one crate into one program.
- A dependency's key in `[dependencies]` is the name the depending package uses. A `package` key
  names the real package when the two differ, so one package can use two series of `json` as
  `json` and `json1`.
- The host's modules come first, as ADR-0036 says. A package named after one of them, or with a
  dependency named after one, is refused when the host installs the packages, so a host module
  never hides a package without anyone noticing.
- A name whose first part isn't a namespace raises `module 'x' not found`, with a hint that says
  which manifest would need to list it (ADR-0029: runtime errors with a hint). A private module
  raises `module 'json.lexer' is private to package 'json'`.
- Each module loads once per state for each package version, in a cache of its own. Scripts can't
  read or replace that cache, even when `package` is granted, so one package can't swap another's
  modules. `package.loaded` holds only what `package`'s own searchers load.
- Code outside every package, such as a chunk the host loads directly, finds the host's modules,
  then the root and exported modules of each installed package by package name, then, when
  `package` is granted, `package`'s searchers.

## The manifest: `Lunaferra.toml`

The manifest is a TOML 1.0.0 file with Cargo's section names and Cargo's style of keys: lowercase
words joined by `-`. `lunaferra-toml` reads it ([`toml.md`](toml.md)). TOML holds only data, with
no expressions and no calls, so reading a manifest can't run code, read the environment, or depend
on the machine it is read on.

```toml
[package]
name = "game"
version = "0.3.1"
description = "The rules and AI of a tile-based strategy game"
license = "MIT"
lunaferra-version = "0.1"   # the Lunaferra releases it runs on
exports = ["rules", "ai"]   # modules other packages may require, besides the root module

[dependencies]
json = { git = "https://github.com/example/lf-json", version = "1.4" }
util = { path = "../util" }

[dev-dependencies]
check = { git = "https://github.com/example/lf-check", tag = "v2.0.0" }

# Severities for `lunaferra check`, `lint`, and `sast`, by diagnostic code.
[lints]
L0001 = "off"

# Settings for the tools.
[tools]
strict = true

[tools.format]
line-width = 100
```

| Key | Value | Meaning |
| :- | :- | :- |
| `package.name` | String | The package's name; required in a package |
| `package.version` | String | Its version under Semantic Versioning; required in a package |
| `package.description` | String | One sentence for people choosing a package |
| `package.license` | String | A Software Package Data Exchange (SPDX) license expression, such as `MIT OR Apache-2.0` |
| `package.lunaferra-version` | String | A version requirement on Lunaferra itself, as Cargo's `rust-version` is on Rust; a release that doesn't meet it refuses the package, with a message that names this key |
| `package.exports` | Array of module names | The modules other packages may require, besides the root module |
| `[dependencies]` | Table of dependencies | The packages the package's modules use, by the name they use |
| `[dev-dependencies]` | Table of dependencies | Packages only `tests/` and `examples/` use; dependents never fetch them |
| `[workspace]` | Table | Makes this manifest a workspace root, as [Workspaces](#workspaces) describes |
| `[lints]` | Table of codes | Each diagnostic code's severity: `"off"`, `"note"`, `"warning"`, or `"error"` ([`diagnostics.md`](diagnostics.md#configuration-lints-and-tools-in-the-manifest)) |
| `[tools]` | Table | The tool settings that [`diagnostics.md`](diagnostics.md#configuration-lints-and-tools-in-the-manifest) lists |

A dependency is an inline table, as in Cargo, with one source and, for a Git source, at most one
way to pick a commit:

| Key | Meaning |
| :- | :- |
| `path` | A folder, relative to the manifest, that holds the package's manifest |
| `git` | A repository URL: `https://`, `ssh://`, or `file://` |
| `version` | A version requirement. With `git`, it picks among the repository's version tags; with `path`, the package's version must meet it |
| `tag`, `branch`, `rev` | With `git` and no `version`: a tag, a branch, or a full commit hash |
| `package` | The real package's name, when the key is another name |
| `workspace` | `true` takes the dependency from `[workspace.dependencies]` |

- In Cargo, a bare string such as `json = "1.4"` means a version from crates.io. Lunaferra has no
  registry yet, so a bare string is an error whose hint says to add `git` or `path`.
- A package name is 1 to 64 bytes of lowercase letters, digits, and `_`, starting with a letter,
  so it can be the first part of a module name. The names of the standard libraries, such as
  `string`, `table`, `list`, and `package`, are reserved, because `require("string")` returns the
  `string` library.
- A key the format doesn't define is an error, with a hint naming the closest key that exists, so
  a typo such as `[dev_dependencies]` or `dependancies` can't drop a dependency without anyone
  noticing.
- A manifest with no `[package]` table is a virtual manifest. It can hold `[workspace]`, `[lints]`,
  or `[tools]`, so a folder of loose scripts still has one place for its tool settings.
- Errors in a manifest, a lock file, or a config file are diagnostics with `M` codes, at the span
  of the key or value that is wrong ([`diagnostics.md`](diagnostics.md#codes)).

## Workspaces

A workspace is a set of packages developed together, as in Cargo. Its root manifest lists the
member folders, and can declare what its members share:

```toml
[workspace]
members = ["game", "packages/*"]

[workspace.package]
version = "0.3.1"
license = "MIT"

[workspace.dependencies]
json = { git = "https://github.com/example/lf-json", version = "1.4" }

[workspace.lints]
L0001 = "off"
```

A member takes what it shares with `workspace = true`, as a Cargo member does:

```toml
[package]
name = "game"
version.workspace = true
license.workspace = true

[dependencies]
json.workspace = true
util = { path = "../util" }

[lints]
workspace = true
```

- A pattern in `members` may end in `*`, which matches each folder at that level that holds a
  manifest.
- Members share one `Lunaferra.lock`, at the workspace root, so they agree on every version.
- Members depend on each other by `path`.
- `[workspace.package]` can hold `version`, `license`, and `lunaferra-version`. A member that sets
  `workspace = true` on a key the root doesn't define is an error.
- `[workspace.lints]` and `[workspace.tools]` reach only the members whose `[lints]` or `[tools]`
  says `workspace = true`, so a member never inherits a setting without saying so.

## Versions and requirements

Versions follow [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html) (SemVer): three
numbers, `major.minor.patch`, where a new major number means a change that breaks dependents, and
an optional pre-release part after `-`, such as `1.0.0-beta.2`. Build metadata after `+` is kept
and ignored in comparisons, as SemVer says.

A version requirement says which versions a dependency may have. It is written as Cargo writes
them:

| Requirement | Matches |
| :- | :- |
| `"1.4"` or `"^1.4"` | At least 1.4.0 and below 2.0.0 |
| `"0.4"` | At least 0.4.0 and below 0.5.0, because before 1.0.0 the minor number marks breaking changes |
| `"~1.4.2"` | At least 1.4.2 and below 1.5.0 |
| `"=1.4.2"` | Only 1.4.2 |
| `">=1.2, <1.7"` | Every version that meets each part |

- Two versions are compatible when they agree on every number of `major.minor.patch` up to and
  including the leftmost one that isn't zero; each set of compatible versions is a series. `1.2.0`
  and `1.9.3` are one series, `0.4.1` and `0.4.9` are one, and `0.4.1` and `0.5.0` are two.
- A requirement matches a pre-release only when it names a pre-release of the same
  `major.minor.patch`, so `"1.4"` never picks `2.0.0-beta.1` or `1.5.0-rc.1`.

## Sources

- **Path.** The package in a folder. A path dependency always uses the folder's current files, and
  the lock file records its version but no source, as `Cargo.lock` does.
- **Git.** A package at the root of a Git repository, at one commit. `rev` names a full commit
  hash, 40 hexadecimal digits for a SHA-1 repository or 64 for a SHA-256 one; a short hash is
  refused, because it can match more than one commit. `tag` and `branch` name the commit that the
  tag or branch points to when the resolver runs. `version` picks the newest tag named
  `v<version>` or `<version>` that meets the requirement, and the commit it names must hold a
  manifest with that version, or the tag is skipped with a warning that names it.

A registry, a service that finds packages by name instead of by URL as crates.io does, is in the
backlog. ADR-0039 plans it as a Git repository that records each package version's Git URL and
commit, so it needs no transport beyond the one Git dependencies use.

## Resolution

The resolver chooses one version for each dependency series in the graph, from the requirements
in the manifests and the versions each source has:

1. It starts from the requirements of the workspace's members, and visits dependencies in sorted
   order of name, so the result never depends on hash order.
2. For each requirement, it picks the newest version that meets every requirement seen so far on
   that series. Versions in different series of one package are chosen apart and can both be in
   the graph.
3. When a later requirement rules out a version already chosen, it goes back to the most recent
   choice that has another candidate, tries that candidate, and continues.
4. It stops with a conflict error when no candidate is left, naming each requirement on the
   package and the manifest that made it, or with a step-limit error after the number of steps
   in [Limits](#limits).
5. When a lock file exists, its versions are tried first, so an unchanged manifest gives an
   unchanged lock file. `lunaferra update` drops them, for every package or for the ones it names.

Backtracking can take a number of steps that grows exponentially with the graph on a crafted
input, so the step limit makes a hostile set of manifests fail fast instead of hang.

## The lock file: `Lunaferra.lock`

`lunaferra fetch` writes the lock file at the root of a package or workspace, and the project
commits it. It is TOML in the shape of `Cargo.lock`: a `version` key for the file's format, then
one `[[package]]` table for each package the workspace's members need, dev-dependencies included.

```toml
# Written by `lunaferra fetch`. Edit the manifests instead of this file.
version = 1

[[package]]
name = "check"
version = "2.0.0"
source = "git+https://github.com/example/lf-check?tag=v2.0.0#52d7e0c4a19f3b6e8d2c5a7f0b4e9d1c3a6f8b2e"

[[package]]
name = "game"
version = "0.3.1"
dependencies = [
  "check",
  "json",
  "util",
]

[[package]]
name = "json"
version = "1.4.2"
source = "git+https://github.com/example/lf-json?tag=v1.4.2#9c1f5e0d2a7b4c3e8f6a1d0b5e2c7f4a9b3d6e8c"

[[package]]
name = "util"
version = "0.2.0"
```

- A Git package's `source` is written as Cargo writes it: `git+`, the URL, the tag, branch, or
  rev it was picked by, and `#` with the full commit hash. A path package has no `source`.
- Each `dependencies` entry is a package's name, or its name and version, such as `"json 1.4.2"`,
  when the lock file holds more than one version of that name, as in `Cargo.lock`.
- Packages are sorted by name and then version, each `dependencies` array is sorted, and the
  writer of [`toml.md`](toml.md#the-writer) lays out every file the same way, so the file changes
  only where the resolution does, and the same resolution gives the same bytes on every platform.
- `version` is the lock file's format. A release that can't read a format refuses the file with a
  message naming the release that can.
- With `--locked`, a command that would change the lock file fails instead, naming the first
  package that would change. CI runs with `--locked`, so a stale lock file fails the build.

## Configuration: `config.toml`

The package commands read settings from `config.toml` files, as Cargo reads `.cargo/config.toml`:

1. `.lunaferra/config.toml` in the current folder and in each folder above it, nearest first;
2. `config.toml` in the cache folder, `LUNAFERRA_HOME` or `.lunaferra` in the home folder.

A key in a nearer file wins over the same key in a farther one, a file read twice counts once, and
a command-line flag wins over every file.

```toml
[net]
offline = true        # as if every command had --offline

[source]
vendor = "vendor"     # read packages from this folder, which lunaferra vendor wrote
```

- These are the only keys. No key names a program, so a config file in a project someone else
  wrote can't make `lunaferra` run anything but `git`.
- `source.vendor` is relative to the folder that holds `.lunaferra/`. With it, every command reads
  locked packages from that folder and never fetches; a package missing from it is an error that
  names the package.

## Fetching with Git

`lunaferra-cli` fetches Git dependencies by running the `git` command, which is part of the
platform ([`02-dependency-policy.md`](../02-dependency-policy.md)). Git speaks HTTPS and SSH, and
names every object by a hash of its contents, so a commit hash pins every file under it. Lunaferra
holds no network code and no cryptography of its own (ADR-0039). This code lives in the
command-line tool, not in `lunaferra-package`, so no host's build contains code that starts other
programs.

- **The cache.** Each repository is fetched once, as a bare repository (one with no working files),
  into `git/` under the cache folder: `LUNAFERRA_HOME`, or `.lunaferra` in the user's home folder.
  Each locked commit's files are written to `checkouts/<name>-<commit>/`, which every project on
  the machine shares.
- **Only data reaches the disk.** Lunaferra never asks Git to check out files. It lists the
  commit's files with `git ls-tree` and reads them with `git cat-file --batch`, then writes each
  one itself. It writes only regular files: never a symbolic link or a submodule, and never a path
  that is absolute, holds `..`, holds a `.git` part, or differs only in case from another path in
  the commit, which would overwrite it on Windows and macOS.
- **Checked objects.** Lunaferra turns on Git's `transfer.fsckObjects`, so Git checks each object
  it receives for damage or malformed contents before storing it.
- **No command from a URL.** Lunaferra starts `git` directly, never through a shell. It passes the
  configuration that allows only the `https`, `ssh`, and `file` protocols, so a URL such as
  `ext::sh -c ...`, which Git would run as a command, is refused. It writes `--` before every URL
  and ref, so a URL that starts with `-` can't become one of Git's options. It refuses a URL or a
  ref with a control character before Git runs.
- **No hooks or prompts.** Git runs no hooks on a fetch, fetches no submodules, and is told not to
  ask for a password at the terminal, so a fetch never waits on a prompt.
- **Paths stay inside.** A `path` dependency in a package fetched from Git must name a folder
  inside the same repository. A path that leaves it is refused before anything is read.
- With `--offline`, no command starts Git's network operations: a commit missing from the cache is
  an error that names the package.

`lunaferra vendor` copies every locked package's manifest and `src/` into
`vendor/<name>-<version>/`, then prints the `[source]` lines to add to `.lunaferra/config.toml`, as
`cargo vendor` does. A project can then build with no network, and a host can bundle its packages.
A host on iOS or the web, where there is no `git` to run, ships the vendored files inside its
binary.

## How a state uses the graph

The embedding API turns a lock file and the packages' files into a `Packages` value that a host
installs in a state ([`lunaferra.md`](lunaferra.md#the-api)):

- A host reads each package's files through a loader it supplies, from a folder or from sources
  compiled into its binary with `include_str!`, so the same packages run on every platform.
- Each chunk records the package it belongs to when it is loaded, and `require` resolves names in
  the namespaces of the package that holds the calling function. The package is set by the loader,
  never stored in the bytecode, so a precompiled chunk can't claim another package's namespace.
- `lunaferra-stdlib`'s `require` asks a resolver trait that the standard library defines, and the
  `lunaferra` crate implements that trait with the module graph, so the VM and the standard library
  build without `lunaferra-package`.

The analysis tools build the same graph from the same files, so the type checker, the linter, and
the language server resolve each `require` to the module the VM would load
([`analysis.md`](analysis.md#modules)).

## The API

The sketch shows the shape; M14.1 to M14.4 settle the names.

```rust
pub struct Manifest { /* package, dependencies, dev_dependencies, workspace, lints, tools */ }
pub struct Lock { /* sorted packages: name, version, source, dependencies */ }
pub struct Config { /* net.offline, source.vendor */ }
pub struct Version { /* major, minor, patch, pre-release, build metadata */ }
pub struct VersionReq { /* the comparators of one requirement */ }

impl Manifest {
    pub fn parse(source: &str, limits: &Limits) -> Result<Manifest, Vec<Diagnostic>>;
}

/// What the resolver asks about the versions a source has. The command-line tool implements it
/// with Git, and tests with packages held in memory.
pub trait Registry {
    fn versions(&mut self, source: &Source) -> Result<Vec<Version>, Error>;
    fn manifest(&mut self, source: &Source, version: &Version) -> Result<Manifest, Error>;
}

pub fn resolve(
    root: &Workspace,
    registry: &mut impl Registry,
    lock: Option<&Lock>,
    limits: &Limits,
) -> Result<Lock, ResolveError>;

/// The packages of one lock file, with each package's namespaces and modules.
pub struct ModuleGraph { /* ... */ }

impl ModuleGraph {
    pub fn resolve(&self, from: PackageId, name: &str) -> Result<ModuleRef, RequireError>;
}
```

## Limits

Manifests and lock files of dependencies come from other people, so every count has a limit with
a named default, and each limit has a test one past its value. These defaults are proposed;
M14.1 and M14.3 set them.

| Limit | Default |
| :- | :- |
| Size of a manifest, a lock file, or a config file | 1 MB |
| Nesting depth of the arrays and tables in a manifest | 16 |
| Dependencies of one package | 1,000 |
| Packages in one resolved graph | 10,000 |
| Resolver steps | 1,000,000 |
| Length of a version or a version requirement | 256 bytes |
| Files in one fetched package | 100,000 |
| Bytes in one fetched package | 256 MB |

## Invariants

1. A manifest, a lock file, or a config file that isn't TOML 1.0.0, holds a key this spec doesn't
   define, or gives a value of the wrong type is rejected with an `M` diagnostic at its span.
2. A module can require only its own package's modules, the root and exported modules of the
   packages its manifest lists, and the host's modules.
3. Every version the resolver chooses meets every requirement on its series in the graph.
4. The same manifests, the same available versions, and the same lock file give the same lock
   file, byte for byte, on every platform.
5. Resolution stops within its step limit on every input.
6. Fetching, vendoring, or resolving a package runs none of its code.
7. Nothing a fetched package holds reaches the disk outside its own folder in the cache, and
   nothing reaches it but regular files.
8. With `--locked`, no command changes `Lunaferra.lock`; with `--offline`, no command starts a
   network operation.
9. No file this spec defines names a program, so `git` is the only program a package command
   starts.

## Tests

- Invariant 1: hostile-input tests in `crates/package/tests/hostile_manifest.rs` give manifests,
  lock files, and config files with unknown keys, `[dev_dependencies]` for `[dev-dependencies]`,
  a bare version string, values of the wrong type, `workspace = true` on a key the root lacks, and
  each limit one past its value, and expect each exact diagnostic; the `manifest` and `lock` fuzz
  targets. [`toml.md`](toml.md#tests) tests TOML itself.
- Invariant 2: integration tests in `crates/lunaferra/tests/packages.rs` require a private module
  of another package, a package the manifest doesn't list, and a host module, each in a sandboxed
  state, and check each result and message.
- Invariants 3 to 5: property tests generate dependency graphs and available versions, resolve
  them, and check every requirement, resolve again and compare the bytes, and give crafted graphs
  that must stop at the step limit with its error.
- Invariant 6: an end-to-end test fetches and vendors a package whose `src/init.lua` writes a file
  if it runs, and checks that no file appears.
- Invariant 7: hostile-input tests in `crates/cli/tests/hostile_git.rs` build local repositories
  with a symbolic link, a submodule, a path with `..`, two paths that differ only in case, and
  more files than the limit, and check that each is refused; others give URLs that start with `-`
  or use `ext::`, and check that Git never runs.
- Invariant 8: end-to-end tests run every package command with `--locked` and compare the lock
  file before and after. With `--offline`, the tests first move the remote repositories away, and
  each command either succeeds from the cache or fails naming the missing package.
- Invariant 9: a test compares the keys the config reader accepts with the table in
  [Configuration](#configuration-configtoml), so a new key needs a change to this spec.
- The `version` fuzz target, and a property test that parsing a version's text gives the same
  version back.
