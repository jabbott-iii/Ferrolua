# Subsystem: `ferrolua-stdlib`

`ferrolua-stdlib` implements Lua 5.5's standard libraries, §6 of the manual: the base library,
`coroutine`, `package`, `string`, `utf8`, `table`, `math`, `io`, `os`, and `debug`. Each function
returns what PUC-Rio Lua 5.5.1 returns and raises the errors it raises, word for word, apart from
the intended differences in [`memory.md`](memory.md) (`collectgarbage`) and
[`sandbox.md`](sandbox.md) (granted libraries, `os.exit`, and file access), and object IDs where
PUC-Rio prints addresses ([Object IDs in output](#object-ids-in-output)). Every function that can
run long counts its work against the instruction limit. The base library lands in M2.13, and the
rest in M3.

## Why port behavior and never code

Scripts depend on exact output: the digits `string.format("%g", x)` prints, the matches a pattern
finds, the order `table.sort` leaves equal elements in. Matching PUC-Rio means reproducing its
behavior, which conformance scripts record. ADR-0002 (no third-party code) means Ferrolua writes
each function itself instead of copying PUC-Rio's C. A task may read PUC-Rio's source to learn what
it does, and its conformance scripts then prove the Rust version does the same.

## Arguments and errors

Every function checks its arguments as PUC-Rio's `luaL_check*` functions do, with the same
messages, such as `bad argument #1 to 'insert' (table expected, got nil)`. A helper module
gives every library the same checks, so the wording can't drift between them.

## Libraries

| Library | Notes |
| :- | :- |
| Base | `print` writes through the host's output sink; `load` and `loadfile` accept mode `"t"`, and `"b"` only when the host allows precompiled chunks; `collectgarbage` follows [`memory.md`](memory.md#collectgarbage); `_VERSION` is `"Lua 5.5"` |
| `coroutine` | Every function in §6.2, including `close` and `isyieldable` |
| `package` | `require`, `package.loaded`, `package.preload`, `package.path`, `package.searchpath`, and a Lua searcher that reads files through the host's loader; `package.loadlib` returns `nil` and an error message, and `package.cpath` has no searcher (ADR-0017) |
| `string` | Patterns per §6.4.1, `format` with every conversion PUC-Rio supports, `pack`, `packsize`, and `unpack` with every format option, and `dump` in Ferrolua's chunk format |
| `utf8` | Every function, with Lua 5.5's `lax` argument and the end position from `utf8.offset` |
| `table` | `create`, `concat`, `insert`, `move`, `pack`, `remove`, `sort`, and `unpack`; `sort` uses PUC-Rio's algorithm, so equal elements end up in the same order and `invalid order function for sorting` is raised in the same cases |
| `math` | `math.random` uses xoshiro256**, seeded as PUC-Rio seeds it, so `math.randomseed(42)` gives the same sequence; `math.randomseed()` with no argument takes its seed from `RandomState` |
| `io` | File handles as userdata with `__close` and `__gc`, shown by their IDs; opened only under the file access policy |
| `os` | Time functions always; the rest only when granted; `os.exit` returns an exit error to the host |
| `debug` | Every function in §6.10, working on Ferrolua's frames, locals, and upvalues; `upvalueid` returns an integer ID |

## Number formatting

`tostring` on a float, `string.format` with `%a`, `%e`, `%f`, and `%g`, and `%q` all print the
exact characters PUC-Rio prints, which comes from the C library's `printf` on Linux. Ferrolua
formats floats itself, with exact decimal conversion so rounding matches in every case, including
subnormal numbers, `inf`, `-inf`, `nan`, and `-nan`.

## Object IDs in output

Where PUC-Rio Lua prints an address, Ferrolua prints the object's ID from
[`memory.md`](memory.md#object-ids), written `#` followed by the number in decimal (ADR-0019).
The `#` keeps an ID from reading as a number or as an address.

| Where | PUC-Rio prints | Ferrolua prints |
| :- | :- | :- |
| `tostring` and `print` of a table, function, thread, or userdata without `__tostring` | `table: 0x5581f1a3c2a0` | `table: #42` |
| The same, with a string `__name` in the metatable | `Point: 0x5581f1a3c2a0` | `Point: #42` |
| `tostring` of an open file handle | `file (0x5581f1a3c2a0)` | `file (#42)` |
| `string.format("%p", v)` for a table, function, thread, userdata, or string | `0x5581f1a3c2a0` | `#42` |
| `debug.upvalueid(f, n)` | A light userdata | The upvalue's ID, as an integer |

- The text before the ID is the text PUC-Rio Lua 5.5.1 prints before the address, for every type
  and for Lua and native functions alike. M2.13 records it here.
- For a value with no address, such as `nil` or a number, `%p` prints what PUC-Rio Lua 5.5.1 prints,
  which M3.4 records here.

## Invariants

1. Every standard library function returns and raises what PUC-Rio Lua 5.5.1 does for the same
   arguments.
2. Every function that can run long stops at the instruction limit.
3. A library the host didn't grant has no function in the state.
4. Formatting any float with `%.17g` and reading it back with `tonumber` gives the same float.
5. No result of any library function contains a memory address.

## Tests

- Invariant 1: one conformance script per library, `tests/conformance/string_format.lua` and so
  on, with argument errors caught by `pcall` and printed.
- Invariant 2: the limit tests in [`sandbox.md`](sandbox.md#tests).
- Invariant 3: the allowlist test in `hostile_sandbox.rs`.
- Invariant 4: a property test on generated bit patterns, including every class of special value.
- Invariant 5: `tests/extensions/object_ids.lua` prints the formats in the table above, and a
  test runs every conformance script and checks that no output contains `0x` followed by 12 or
  more hexadecimal digits.
