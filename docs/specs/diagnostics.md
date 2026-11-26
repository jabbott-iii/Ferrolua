# Subsystem: `diagnostics`

This spec is the contract every Lunaferra tool follows when it reports a problem in Lua code: the
syntax checker, strict mode, the type checker, the linter, the formatter, the SAST tool, and the
package manager. It defines the diagnostic type, the codes, the output formats, suppression
comments, and the tool settings in the package manifest, `Lunaferra.toml` (ADR-0038, proposed:
packages and modules, with TOML files as Cargo uses them).
`lunaferra-syntax`, which every tool depends on, defines the type, and each tool adds its codes as
it lands.

## Why one contract

A user meets every tool through the same terminal, editor, and CI log. One diagnostic shape lets
the command line print them alike, lets the language server send them to any editor, and lets
SARIF upload them to code scanning. Stable codes let a project turn a rule off by name, and keep
that setting working across releases: renaming or reusing a code is a breaking change, as
`.claude/rules/commits.md` says.

## The diagnostic

Built in M1.6 as described here, in `crates/syntax/src/diagnostic.rs`:

```rust
#[non_exhaustive]
pub struct Diagnostic {
    pub code: Code,             // such as L0003
    pub severity: Severity,     // Error, Warning, or Note
    pub message: String,        // one sentence, no trailing period
    pub span: Span,             // file and byte range of the main problem
    pub labels: Vec<Label>,     // other spans, each with a short message
    pub notes: Vec<String>,     // help and background
    pub fixes: Vec<Fix>,        // text edits that resolve it, when one is certain
}
impl Diagnostic {
    pub fn new(code: Code, severity: Severity, message: impl Into<String>, span: Span) -> Self;
}

pub struct Code { /* a Tool and a number from 1 to 9,999 */ }   // displays as P0013
impl Code {
    pub const fn new(tool: Tool, number: u16) -> Code;   // panics outside 1..=9,999
}
pub enum Tool { Parser, Compiler, TypeChecker, Linter, Formatter, Sast, Manifest }   // the letter
pub enum Severity { Error, Warning, Note }

pub struct Label { pub span: Span, pub message: String }
pub struct Fix { pub message: String, pub edits: Vec<Edit> }
pub struct Edit { pub span: Span, pub text: Vec<u8> }   // replaces the span's bytes
```

`Diagnostic` is marked `#[non_exhaustive]`, so another crate can't build one from its fields, and
a field added later breaks no tool. A tool makes one with `Diagnostic::new`, which leaves the
labels, notes, and fixes empty, and then adds to them. Each `Tool` gives its codes their letter,
from the table below. `Code::new` panics on a number out of range, and Rust evaluates each
`const` item while it compiles, so a `const` code with such a number fails to compile. An edit's
text is bytes, like the source it goes in, which need not be valid UTF-8.

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

Each kind of lexical and syntax error in `lunaferra-syntax` has a `P` code of its own (the owner's
choice, 2026-11-26), so a code can have a page of its own that explains it. The lexer's kinds come
first, then the parser's, each in the order its enum lists them. Each message is PUC-Rio Lua
5.5.1's where the syntax is Lua's; [`syntax.md`](syntax.md#syntax-errors) says how the diagnostics
are made from the errors.

### Registry

| Code | Name | Meaning | Lands in |
| :- | :- | :- | :- |
| `P0001` | `malformed-number` | `malformed number`: a numeral that is neither an integer nor a float, such as `0x` | M1.6 |
| `P0002` | `unfinished-string` | `unfinished string`: a short string or f-string with no closing quote before its line or the source ends | M1.6 |
| `P0003` | `unfinished-long-string` | `unfinished long string (starting at line N)`: a long string or f-string with no closing bracket | M1.6 |
| `P0004` | `unfinished-long-comment` | `unfinished long comment (starting at line N)`: a long comment with no closing bracket | M1.6 |
| `P0005` | `invalid-long-string-delimiter` | `invalid long string delimiter`: `[` and `=` signs without a second `[` | M1.6 |
| `P0006` | `invalid-escape` | `invalid escape sequence`: a backslash that starts no escape, such as `\q` | M1.6 |
| `P0007` | `hex-digit-expected` | `hexadecimal digit expected`: `\x` without two hexadecimal digits, or `\u{` without one | M1.6 |
| `P0008` | `missing-opening-brace` | `missing '{'`: `\u` without `{` | M1.6 |
| `P0009` | `missing-closing-brace` | `missing '}'`: `\u{` and its digits without `}` | M1.6 |
| `P0010` | `utf8-value-too-large` | `UTF-8 value too large`: `\u{...}` with a value above `7FFFFFFF` | M1.6 |
| `P0011` | `decimal-escape-too-large` | `decimal escape too large`: `\ddd` with a value above 255 | M1.6 |
| `P0012` | `single-closing-brace` | `single '}' in f-string`: a `}` in an f-string's text that isn't part of `}}` and ends no field | M1.6 |
| `P0013` | `expected` | `X expected`, such as `'then' expected`: a token of that kind must come next | M1.6 |
| `P0014` | `unclosed` | `X expected (to close Y at line N)`: the token that closes a construct opened on an earlier line must come next | M1.6 |
| `P0015` | `unexpected-symbol` | `unexpected symbol`: a token that can start no expression where one must start, or no statement | M1.6 |
| `P0016` | `syntax-error` | `syntax error`: a statement that is an expression but neither a call nor an assignment, or an assignment to something that isn't a name, a field, or an index | M1.6 |
| `P0017` | `equal-or-in-expected` | `'=' or 'in' expected`: a `for` loop's first name followed by neither | M1.6 |
| `P0018` | `name-or-vararg-expected` | `<name> or '...' expected`: a parameter that is neither | M1.6 |
| `P0019` | `function-arguments-expected` | `function arguments expected`: a method's name after `:` without arguments | M1.6 |
| `P0020` | `break-outside-loop` | `break outside loop`: a `break` that no loop of its function holds | M1.6 |
| `P0021` | `vararg-outside-vararg-function` | `cannot use '...' outside a vararg function` | M1.6 |
| `P0022` | `unknown-attribute` | `unknown attribute 'x'`: an attribute other than `const` and `close` | M1.6 |
| `P0023` | `multiple-to-be-closed` | `multiple to-be-closed variables in local list`: two `<close>` variables in one `local` statement | M1.6 |
| `P0024` | `global-to-be-closed` | `global variables cannot be to-be-closed`: a `<close>` attribute in a `global` declaration | M1.6 |
| `P0025` | `lua-concatenation` | `'..' is not an operator: use '+' to join strings`: Lua's `..` after an expression | M1.6 |
| `P0026` | `lua-not-equal` | `'~=' is not an operator: use '!=' to test inequality`: Lua's `~=`, with a fix that writes `!=` | M1.6 |
| `P0027` | `lua-bitwise-xor` | `'~' is not a binary operator: use '^' for bitwise XOR`: Lua's binary `~`, with a fix that writes `^` | M1.6 |
| `P0028` | `nesting-too-deep` | `C stack overflow`: blocks and expressions nested one level past the parser's limit of 197 levels | M1.8 |

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

## Configuration: `[lints]` and `[tools]` in the manifest

The tools read their settings from the manifest, `Lunaferra.toml`, in the directory they run in or
the nearest parent that has one (ADR-0038). `[lints]` sets each diagnostic code's severity, as
Cargo's `[lints]` table does, and `[tools]` holds every other setting. A manifest with no
`[package]` table holds settings for loose scripts. The manifest is TOML
([`package.md`](package.md#the-manifest-lunaferratoml)), which holds only data, so reading it runs
no code. In a workspace, a member takes the root's `[workspace.lints]` or `[workspace.tools]` when
its own `[lints]` or `[tools]` says `workspace = true` ([`package.md`](package.md#workspaces)).

```toml
[package]
name = "game"
version = "0.3.1"

# Each code's severity: "off", "note", "warning", or "error".
[lints]
L0001 = "off"
S0002 = "error"

[tools]
# Files for the tools to check; by default, a package's src/, tests/, and examples/.
include = ["src/**/*.lua", "scripts/*.lua"]
exclude = ["src/vendor/**"]

# Where loose scripts find modules; a package's modules follow its module graph instead.
package-path = "./?.lua;./?/init.lua"

# Names the host defines, for strict mode and the type checker.
globals = ["game", "log"]
declarations = ["types/game.d.lua"]
strict = true

[tools.format]
indent-width = 2
line-width = 100
quote-style = "double"

[tools.sast]
sources = ["game.read_chat", "http.request_body"]
sinks = ["game.run_command"]
```

- `P`, `C`, and `M` codes are always errors, so a `[lints]` entry for one is itself an error.
- `lunaferra-package` reads the manifest in M14.1, and `lunaferra-analysis` takes the settings from
  it in M6.3; each tool's spec lists the settings it adds. Command-line flags override the manifest.

## Invariants

1. Every code a tool emits is in the registry, and no two rows share a code.
2. Every diagnostic's span lies inside its file.
3. JSON Lines output parses with `lunaferra-json`, one object per line, and SARIF output is a valid
   SARIF 2.1.0 log.
4. A suppression comment removes exactly the codes it names, on exactly the lines it covers.
5. A manifest with a key or a value the format doesn't define, or a `[lints]` entry for a `P`,
   `C`, or `M` code, is rejected with an `M` diagnostic at its span.

## Tests

- Invariant 1: a test in `lunaferra-cli` collects every code each tool can emit and compares the
  list with the registry table in this file. For the `P` codes, a unit test in
  `crates/syntax/src/source_error.rs` already compares the code of each kind of lexical and syntax
  error with the `P` rows of the registry, in order.
- Invariant 2: a property test runs every tool on generated programs, valid and broken, and checks
  each span.
- Invariant 3: tests parse the JSON Lines output with `lunaferra-json`, and check SARIF logs for the
  fields that SARIF 2.1.0 requires.
- Invariant 4: tests cover each comment form, a suppression of an absent code, and a suppression
  of a `P` code.
- Invariant 5: hostile-input tests in `lunaferra-package` give manifests with unknown keys, values
  of the wrong type, and `[lints]` entries for `P`, `C`, and `M` codes, and expect each exact
  diagnostic ([`package.md`](package.md#tests)).
