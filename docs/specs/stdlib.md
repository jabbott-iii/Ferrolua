# Subsystem: `ferrolua-stdlib`

`ferrolua-stdlib` implements Lua 5.5's standard libraries, §6 of the manual: the base library,
`coroutine`, `package`, `string`, `utf8`, `table`, `math`, `io`, `os`, and `debug`. Positions count
from 0, ranges stop before their end, and no argument converts between strings and numbers
(ADR-0023: a language of its own, with Python's operators and 0-based indexes). Apart from those
changes, each function returns what PUC-Rio Lua 5.5.1 returns and raises the errors it raises,
word for word, except for the intended differences in [`memory.md`](memory.md) (`collectgarbage`)
and [`sandbox.md`](sandbox.md) (granted libraries, `os.exit`, and file access), and object IDs
where PUC-Rio prints addresses ([Object IDs in output](#object-ids-in-output)). Every function that
can run long counts its work against the instruction limit. The base library lands in M2.13, and
the rest in M3.

## Why port behavior and never code

Scripts depend on exact output: the digits `string.format("%g", x)` prints, the matches a pattern
finds, the order `table.sort` leaves equal elements in. Matching PUC-Rio means reproducing its
behavior, which conformance scripts record. ADR-0002 (no third-party code) means Ferrolua writes
each function itself instead of copying PUC-Rio's C. A task may read PUC-Rio's source to learn what
it does, and its conformance scripts then prove the Rust version does the same.

## Arguments and errors

Every function checks its arguments as PUC-Rio's `luaL_check*` functions do, with the same
messages, such as `bad argument #1 to 'insert' (table expected, got nil)`, except that it converts
nothing:

- A parameter that takes a number rejects a string, and one that takes a string rejects a number,
  so `string.rep("x", "3")`, `string.len(123)`, and `table.concat({1, 2})` raise errors (ADR-0023).
- A parameter that takes an integer rejects a float, even one with an integer value such as `3.0`
  (ADR-0024: the details Python's operators and 0-based indexes leave open).
- `print`, `tostring`, and `string.format`'s `%s` still turn any value into a string, because that
  is what they are for, and `tonumber` still reads a number from a string.

M2.13 records the message for each rejected argument here. Argument numbers in messages still
count from 1, so `#1` is the first argument. A helper module gives every library the same checks,
so the wording can't drift between them.

## Positions and ranges

A position counts from 0, and a negative position counts back from the end, with -1 the last. A
range from i to j covers i and stops before j, as Python's slices do, and a range that runs past
either end is cut to fit, as in Lua and Python
([`language.md`](language.md#indexes-and-ranges-adr-0023)).

| Function | What its positions name | Defaults |
| :- | :- | :- |
| `select(i, ...)` | The arguments from i on | None |
| `ipairs(t)` | `t[0]`, `t[1]`, and so on, to the first `nil` | |
| `string.sub(s, i, j)` | Bytes i to j − 1 | j is `#s` |
| `string.byte(s, i, j)` | Bytes i to j − 1 | i is 0, j is i + 1 |
| `string.find(s, p, init, plain)` | Searches from `init`, and returns the match's start and its end, so `s:sub(start, stop)` is the match | `init` is 0 |
| `string.match` and `string.gmatch` | Search from `init`; a position capture `()` gives a 0-based position | `init` is 0 |
| `string.unpack(fmt, s, pos)` | Reads from `pos`, and returns the next position after the values | `pos` is 0 |
| `table.insert(t, pos, v)` | Inserts at `pos`, from 0 to `#t` | Without `pos`, appends at `#t` |
| `table.remove(t, pos)` | Removes the element at `pos` | `pos` is `#t - 1` |
| `table.concat(t, sep, i, j)` and `table.unpack(t, i, j)` | Elements i to j − 1 | i is 0, j is `#t` |
| `table.move(a1, f, e, t, a2)` | Copies `a1[f]` to `a1[e - 1]` into `a2` from `a2[t]` on | `a2` is `a1` |
| `table.pack(...)` | Stores the arguments at 0 to n − 1, and n in the field `n` | |
| `utf8.codepoint(s, i, j)` and `utf8.len(s, i, j)` | Characters that start in bytes i to j − 1 | i is 0; j is i + 1 for `codepoint` and `#s` for `len` |
| `utf8.offset(s, n, i)` | The start and the end of character n, counting from byte i; n = 0 is the character that contains byte i, and a negative n counts back | i is 0 when n ≥ 0, and `#s` otherwise |
| `utf8.codes(s)` | Gives each character's 0-based byte position and its code point | |
| `math.random(m, n)` | An integer from m to n − 1; `math.random(n)` is from 0 to n − 1, and `math.random(0)` an integer whose bits are all random (ADR-0024) | |
| `debug.getlocal`, `setlocal`, `getupvalue`, `setupvalue`, `upvalueid`, and `upvaluejoin` | Locals and upvalues from 0; a negative number names a vararg, with -1 the first (ADR-0024) | |

Pattern capture numbers, stack levels, line numbers, `file:seek` offsets, and the fields of
`os.date` and `os.time` don't change: `%1` is still the first capture, and a month is still 1 to
12.

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
   arguments, once each position is moved to count from 0 and each range to stop before its end.
2. Every function that can run long stops at the instruction limit.
3. A library the host didn't grant has no function in the state.
4. Formatting any float with `%.17g` and reading it back with `tonumber` gives the same float.
5. No result of any library function contains a memory address.
6. No library function, apart from `print`, `tostring`, `tonumber`, and `string.format`'s `%s`,
   converts an argument between a string and a number.

## Tests

- Invariant 1: one conformance script per library, `tests/conformance/string_format.lua` and so
  on, each with a Lua 5.5 reference twin that PUC-Rio runs
  ([`04-quality.md`](../04-quality.md#conformance-scripts)), with argument errors caught by `pcall`
  and printed. The positions and ranges in the table above are checked against output written from
  this spec in `tests/extensions/positions.lua`.
- Invariant 2: the limit tests in [`sandbox.md`](sandbox.md#tests).
- Invariant 3: the allowlist test in `hostile_sandbox.rs`.
- Invariant 4: a property test on generated bit patterns, including every class of special value.
- Invariant 5: `tests/extensions/object_ids.lua` prints the formats in the table above, and a
  test runs every conformance script and checks that no output contains `0x` followed by 12 or
  more hexadecimal digits.
- Invariant 6: a test calls every library function with a numeric string where it takes a number,
  and with a number where it takes a string, and checks that each call raises the wrong-type error.
