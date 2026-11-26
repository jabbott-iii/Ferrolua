# Subsystem: `lunaferra-sast`

`lunaferra-sast` is Lunaferra's static application security testing (SAST) tool. It finds security
problems in Lua code without running it: untrusted data that reaches a dangerous function, secrets
written into the source, weak randomness, and code that breaks out of a sandbox. It tracks data
across functions and files with taint analysis, reports `S` diagnostics, and writes SARIF for code
scanning. Hosts declare their own sources and sinks, because a game's chat function or a server's
request body is untrusted in ways only the host knows. It lands in M8.

## Why taint analysis

Most security bugs in scripts are data from outside reaching a function that runs it, as code, as
a shell command, or as a path. Taint analysis marks the values that come from a source, follows them
through assignments, calls, returns, table fields, and string operations, and reports when one
reaches a sink without passing a sanitizer. It finds the bug at its cause, the flow, rather than at
one call. The cost is false positives where a value is checked in a way the analysis doesn't
understand, which a suppression comment documents.

## Sources, sinks, and sanitizers

| Kind | Built in | From the host |
| :- | :- | :- |
| Sources | `io.read`, `io.lines`, file `read`, `os.getenv`, `arg`, and the parameters of functions the host marks as entry points | `sources` in the manifest's `[tools.sast]` |
| Sinks | `load`, which runs text as code; `os.execute` and `io.popen`; `io.open`, `dofile`, `loadfile`, `os.remove`, and `os.rename` paths; `require` with a computed name; pattern arguments to `string.find`, `match`, `gmatch`, and `gsub`; counts given to `string.rep` | `sinks` in `[tools.sast]` |
| Sanitizers | `tonumber`, `math.tointeger`, and comparisons against literal strings | `sast.sanitizers` |

## Rules

Each rule's code is assigned when its task adds it to the registry in
[`diagnostics.md`](diagnostics.md#registry). The first set:

| Rule | Finds |
| :- | :- |
| Code injection | Tainted text reaching `load` |
| Command injection | Tainted text reaching `os.execute` or `io.popen` |
| Path traversal | A tainted path reaching a file function |
| Pattern injection | A tainted pattern, which can make the matcher slow or match more than intended |
| Resource exhaustion | A tainted count reaching `string.rep`, or a tainted size reaching `table.create` |
| Hardcoded secret | A string with high entropy assigned to a name such as `password`, `secret`, `token`, or `api_key` |
| Weak randomness | `math.random` used to build a value named like a secret or a token |
| Sandbox escape | Use of the `debug` library, changes to `getmetatable("")`, `setmetatable` or `rawset` on `_G`, and `load` in binary mode |

## SARIF output

`lunaferra sast --format sarif` writes one SARIF 2.1.0 log with a rule entry for each `S` code and a
result for each finding, including the taint path as a code flow. GitHub's code scanning accepts it
through the `github/codeql-action/upload-sarif` action.

## Invariants

1. Every rule has a test with a flow it reports, and a flow through a sanitizer that it doesn't.
2. A flow through any number of assignments, calls, returns, and table fields is followed, up to the
   analysis depth that `[tools.sast]` sets.
3. Analysis finishes in time bounded by the project's size, including recursive functions.
4. SARIF output is a valid SARIF 2.1.0 log.

## Tests

- Invariant 1: one test file per rule in `crates/sast/tests/`.
- Invariant 2: generated flows of increasing length through each construct.
- Invariant 3: the `sast` fuzz target, and hostile-input tests with deep and mutual recursion.
- Invariant 4: the SARIF checks of [`diagnostics.md`](diagnostics.md#tests).
