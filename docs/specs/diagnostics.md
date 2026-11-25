# Subsystem: `diagnostics`

This spec is the contract every Lunaferra tool follows when it reports a problem in Lua code: the
syntax checker, strict mode, the type checker, the linter, the formatter, the SAST tool, and the
package manager. It defines the diagnostic type, the codes, the output formats, suppression
comments, and the tool settings in the package manifest, `Lunaferra.lua` (ADR-0016, proposed: the
tools read a data-only Lua file; ADR-0038, proposed: that file is the package manifest).
`lunaferra-syntax` defines the type in M1.6, and each tool adds its codes as it lands.

## Why one contract

A user meets every tool through the same terminal, editor, and CI log. One diagnostic shape lets
the command line print them alike, lets the language server send them to any editor, and lets
SARIF upload them to code scanning. Stable codes let a project turn a rule off by name, and keep
that setting working across releases: renaming or reusing a code is a breaking change, as
`.claude/rules/commits.md` says.

## The diagnostic

```rust
pub struct Diagnostic {
    pub code: Code,             // such as L0003
    pub severity: Severity,     // Error, Warning, or Note
    pub message: String,        // one sentence, no trailing period
    pub span: Span,             // file and byte range of the main problem
    pub labels: Vec<Label>,     // other spans, each with a short message
    pub notes: Vec<String>,     // help and background
    pub fixes: Vec<Fix>,        // text edits that resolve it, when one is certain
}
```

## Codes

A code is one letter for the tool and four digits, numbered from `0001` in the order codes are
added:

| Prefix | Tool | Severity |
| :- | :- | :- |
| `P` | Syntax errors from the parser | Always an error |
| `C` | Compile-time checks: strict mode and `<const>` rules | Always an error |
| `T` | Type checker | Error by default; configurable |
| `L` | Linter | Warning by default; configurable |
| `F` | Formatter: a file that isn't formatted | Configurable |
| `S` | SAST | Warning or error by rule; configurable |
| `M` | Package manifests and lock files ([`package.md`](package.md)) | Error, except a skipped version tag, which is a warning |

A code is never renamed, and a removed code's number is never reused. Each task that adds a
diagnostic adds its row to the registry below, in the same change.

### Registry

| Code | Name | Meaning | Lands in |
| :- | :- | :- | :- |
| None yet | | | |

## Output formats

| Format | Flag | Use |
| :- | :- | :- |
| Text | (default) | People: the code, the message, and the source line with the span underlined |
| JSON Lines | `--format json` | Scripts: one JSON object per diagnostic, one per line, with stable field names |
| SARIF 2.1.0 | `--format sarif` | Code scanning: one Static Analysis Results Interchange Format (SARIF) log, which GitHub accepts as an upload |

Runtime errors from a running script aren't diagnostics. They keep PUC-Rio Lua 5.5.1's wording,
because scripts match on them. The `lunaferra` command and the debugger still print them in the text
format: the message, the file, line, and column, the source line with carets under the failing
expression, and a hint (ADR-0029: runtime errors with their source line, a caret, and a hint).

## Suppression comments

```lua
local unused = 1  --lunaferra:allow L0001

--lunaferra:allow L0002, S0004
risky_call()
```

- `--lunaferra:allow <codes>` at the end of a line suppresses those codes on that line, and on a
  line of its own suppresses them on the next line.
- `--lunaferra:allow-file <codes>` before the first statement suppresses them in the whole file.
- A suppression names its codes; there is no "allow everything". A suppression that suppresses
  nothing is itself reported by the linter.
- `P` and `C` codes can't be suppressed, because the file doesn't compile.

## Configuration: the manifest's `tools` table

The tools read their settings from the `tools` table of the manifest, `Lunaferra.lua`, in the
directory they run in or the nearest parent that has one (ADR-0038). A manifest with no `package`
table holds settings for loose scripts. The manifest is a data-only Lua file: one statement,
`return` followed by a table constructor, holding table constructors, strings, numbers, booleans,
and comments, and nothing else; any other expression is an error. The tools parse it and never run
it. In a workspace, a member with no `tools` table of its own uses the root's
([`package.md`](package.md#workspaces)).

```lua
return {
  package = { name = "game", version = "0.3.1" },

  tools = {
    -- Files for the tools to check; by default, a package's src/, tests/, and examples/.
    include = { "src/**/*.lua", "scripts/*.lua" },
    exclude = { "src/vendor/**" },

    -- Where loose scripts find modules; a package's modules follow its module graph instead.
    package_path = "./?.lua;./?/init.lua",

    -- Names the host defines, for strict mode and the type checker.
    globals = { "game", "log" },
    declarations = { "types/game.d.lua" },
    strict = true,

    rules = {
      L0001 = "off",
      S0002 = "error",
    },

    format = {
      indent_width = 2,
      line_width = 100,
      quote_style = "double",
    },

    sast = {
      sources = { "game.read_chat", "http.request_body" },
      sinks = { "game.run_command" },
    },
  },
}
```

`lunaferra-package` reads the manifest in M14.1, and `lunaferra-analysis` takes the tool settings
from it in M6.3; each tool's spec lists the settings it adds. Command-line flags override the
manifest.

## Invariants

1. Every code a tool emits is in the registry, and no two rows share a code.
2. Every diagnostic's span lies inside its file.
3. JSON Lines output parses with `lunaferra-json`, one object per line, and SARIF output is a valid
   SARIF 2.1.0 log.
4. A suppression comment removes exactly the codes it names, on exactly the lines it covers.
5. Reading a manifest runs no code: a file with a function call, an operator, or a name in an
   expression is rejected.

## Tests

- Invariant 1: a test in `lunaferra-cli` collects every code each tool can emit and compares the
  list with the registry table in this file.
- Invariant 2: a property test runs every tool on generated programs, valid and broken, and checks
  each span.
- Invariant 3: tests parse the JSON Lines output with `lunaferra-json`, and check SARIF logs for the
  fields that SARIF 2.1.0 requires.
- Invariant 4: tests cover each comment form, a suppression of an absent code, and a suppression
  of a `P` code.
- Invariant 5: hostile-input tests in `lunaferra-package` give manifests with calls, operators, and
  names, and expect each to be rejected with its exact message
  ([`package.md`](package.md#tests)).
