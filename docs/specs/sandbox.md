# Subsystem: `sandbox`

This spec is the contract between a host and the scripts it runs: which libraries a script can
reach, which limits stop it, and what a hostile script can and can't do. The VM is the sandbox
(ADR-0014, proposed: hosts allowlist libraries, and the VM enforces limits that `pcall` can't
catch). `ferrolua-vm` enforces the limits, `ferrolua-stdlib` honors the library choices, and
`ferrolua` exposes both to hosts. The limits land in M3.2, and the library allowlist in M3.13.

## Why the VM is the sandbox

Hosts embed Lua to run code they didn't write: game mods, user plugins, rules in a server. An
operating system sandbox, such as a separate process, is strong but heavy for a host that runs
thousands of small scripts, and differs on every platform. Ferrolua instead gives a script only the
functions its host grants, and counts every resource a script uses inside the VM, including the
work done inside library functions, so a host never depends on a script's cooperation. The cost is
a check on each counted resource, and a limit error that `pcall` can't catch, unlike PUC-Rio Lua's
catchable `not enough memory`.

## Libraries

A host picks each state's libraries when it creates the state. Nothing outside this table exists in
a state unless the host registers it.

| Library | Default | Notes |
| :- | :- | :- |
| Base | On | `load` reads text only; `dofile` and `loadfile` exist only when file access is granted; `print` writes to the host's output, which defaults to discarding it |
| `string` | On | `string.dump` exists only when precompiled chunks are allowed |
| `table`, `math`, `utf8`, `coroutine` | On | Pure computation |
| `os` time functions | On | `os.time`, `os.clock`, `os.date`, and `os.difftime` |
| Rest of `os` | Off | `os.execute`, `os.remove`, `os.rename`, `os.tmpname`, `os.getenv`, `os.setlocale`, and `os.exit` |
| `io` | Off | Granted with a file access policy, below |
| `package` and `require` | Off | Granted with a module loader the host supplies; never loads C modules (ADR-0017) |
| `debug` | Off | Can read and change any local and upvalue, so it breaks every other boundary |
| Precompiled chunks | Off | `load` with mode `"b"` and `string.dump` (ADR-0010) |

Two presets cover common cases: `Libraries::SAFE`, the defaults above, and `Libraries::ALL`,
everything, which the `ferrolua run` command uses because its user runs their own script.

### File access

A host that grants `io` also picks a policy: no files, read-only under one directory, read-write
under one directory, or unrestricted. Under a directory policy, every path is resolved to an
absolute path with symbolic links followed, and refused unless it is inside the directory. A path
that leaves it, such as `../secret`, raises an error. Between that check and the open, another
process could replace a directory with a symbolic link; Ferrolua documents this race rather than
claim to close it.

### Exiting

`os.exit` never ends the host process. It stops the script and returns an exit error with the
status to the host, which the command-line tool turns into the process's exit status.

## Limits

| Limit | Counts | Default without a host limit |
| :- | :- | :- |
| Memory | Bytes the state has allocated ([`memory.md`](memory.md)) | None; a failed allocation runs the loop collector, then raises a catchable `not enough memory` |
| Instructions | Bytecode instructions run, plus work units inside library calls | None |
| Call depth | Nested calls, Lua and native | PUC-Rio Lua 5.5.1's limits on nested native calls and stack slots, which M3.2 records |
| String size | Bytes in one string | `2^63 - 1`, limited by memory in practice |
| Source size | Bytes of one chunk passed to `load` | None |
| Nesting depth | Nested blocks and expressions in the parser | PUC-Rio Lua 5.5.1's limit, which M1.8 records |

Library functions that can run long count their work against the instruction limit: one unit per
byte that `string.rep`, `table.concat`, and `string.format` produce, and one per step of the
pattern matcher in `string.find`, `string.match`, `string.gmatch`, and `string.gsub`. A pattern
built to make the matcher backtrack is stopped like a loop. Table operations are bounded too
(ADR-0020): if M2.4 chooses to count them, each probe step past the first is one unit.

### When a limit is reached

Before the memory limit stops a script, the VM runs the loop collector once, so garbage loops
never cause a limit error. The VM then stops the script and returns `Error::Limit` to the host,
naming the limit. `pcall` and
`xpcall` don't catch it, so a script can't catch the error and keep going. Pending finalizers and
`<close>` variables still need to run; M3.2 decides, before work starts, whether they run with a
small reserve of instructions or are skipped. After a limit error, the host may raise the limit and
call into the state again.

## Isolation

States share nothing. Each has its own registry, globals, string metatable, loaded modules, and
string hash seed, so one script changing `getmetatable("").__index` affects only its own state.
By default the seed comes from the standard library's `RandomState`, so a script can't predict hash
collisions to slow its table lookups. A host that fixes the seed with `Options::hash_seed` keeps
that defense through the probe bound of ADR-0020: no set of keys makes one table operation run
long. No output reveals a memory address: objects print as IDs
(ADR-0019), so a script learns nothing about where the host's memory is.

## Invariants

1. A script reaches no library function its host didn't grant.
2. Each limit stops the first script that passes it by one unit.
3. A limit error passes through every `pcall` and `xpcall` to the host.
4. Under a directory policy, no path outside the directory opens, whatever its form.
5. `os.exit` never ends the host process.
6. A change to one state's globals, metatables, or modules is invisible to every other state.
7. Nothing a script can print or compute contains a memory address.

## Tests

`crates/ferrolua/tests/hostile_sandbox.rs` holds the escape attempts, each with its exact error:

- Invariant 1: for every function in the full standard library, a script in a `SAFE` state checks
  that it is absent unless the table above lists it.
- Invariant 2: for each limit, scripts at the limit and one past it; for instructions, an infinite
  loop, deep recursion, and a backtracking pattern in each pattern function.
- Invariant 3: limits reached inside `pcall`, inside `xpcall` with a handler that loops, inside a
  metamethod, and inside a coroutine.
- Invariant 4: `..`, absolute paths, symbolic links that leave the directory, empty paths, and paths
  with NUL bytes.
- Invariant 5: `os.exit` with and without a status, inside `pcall` and a coroutine.
- Invariant 6: two states in one test, one changing shared names, the other checking them.
- Invariant 7: scripts pass every kind of object to `tostring`, `print`, `string.format` with
  `%p` and `%s`, `debug.upvalueid`, and error messages, and check that no result contains an
  address of the test process, read from `/proc/self/maps`.
