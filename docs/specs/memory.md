# Subsystem: `memory`

This spec is the contract for how Ferrolua stores and frees Lua values. Every value is reference
counted and freed when its count reaches zero, with no garbage collector (ADR-0003). Values live
in per-state arenas and are reached through generational handles, so the heap is safe Rust and a
counting bug can't read freed memory (ADR-0011). `ferrolua-vm` implements it, starting in M2.2;
`ferrolua-jit` and `ferrolua-stdlib` follow its rules.

## Why reference counting in arenas

A tracing garbage collector, such as PUC-Rio Lua's, frees loops of values, but it pauses at times
the host doesn't choose, and its heap is a web of raw pointers. Reference counting frees each value
at a known point, the way Rust's `Rc` does, but it can't free a loop. Ferrolua accepts that loops
live until their state is dropped, and removes the loops Lua code makes most often: recursion and
classes. Keeping every value in arenas owned by the state means dropping the state frees
everything at once, loops included, without tracing anything.

## Objects and arenas

Each kind of heap object has an arena: a growable array of slots plus a list of free slots.

| Arena | Holds |
| :- | :- |
| Strings | Immutable byte strings, short ones interned (see [`vm.md`](vm.md)) |
| Tables | Array part, hash part, metatable, and weak mode |
| Closures | A Lua function: its prototype and upvalue cells |
| Native functions | A Rust function the host or the standard library registered, with its captured data |
| Upvalue cells | A captured local after its frame returns |
| Userdata | A host's Rust value, with a metatable |
| Threads | A coroutine's stack and frames |
| Classes and instances | A class's methods and field layout, and an instance's fields (ADR-0007) |

Function prototypes, the compiled code of a function, are immutable and shared by every closure
made from them. They are counted the same way.

```rust
struct Slot<T> {
    generation: u32,
    count: u32,
    object: Option<T>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Handle {
    index: u32,
    generation: u32,
}
```

A handle names a slot and the generation it expects. Each time a slot is freed, its generation
goes up by one, so an old handle no longer matches. Reading through a stale handle returns an
internal error, never another object's data.

## Values

```rust
enum Value {
    Nil,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(Handle),
    Table(Handle),
    Function(Handle),
    Userdata(Handle),
    Thread(Handle),
}
```

A `Value` is 16 bytes, the same size as PUC-Rio's. Ferrolua has no light userdata, because it has
no C API (ADR-0017). Classes and instances appear as variants of their own once ADR-0007 settles
how `type` reports them.

## Counting

Every place that holds a `Value` holds a counted reference:

- each register and stack slot;
- each table entry, key and value, except the weak ones;
- each upvalue cell, closure, and metatable link;
- the registry, the string metatable, and the loaded-module table;
- each value the host holds through the embedding API ([`ferrolua.md`](ferrolua.md)).

Copying a value into one of these increments its count, and overwriting or dropping one decrements
it. A count that would pass `u32::MAX` stays at `u32::MAX`, and that object is freed only when the
state is dropped. The memory limit makes this unreachable in practice, because four billion
references need at least 64 GB.

## Freeing

When a count reaches zero:

1. If the object is marked for finalization, it moves to the queue of pending finalizers, which
   holds one count. An object is marked when `setmetatable` gives it a metatable that has `__gc`
   at that moment, as §2.5.3 of the manual describes.
2. Otherwise, the slot is cleared, its generation goes up, and it joins the free list. Each value
   the object held is decremented in turn.

Freeing uses an explicit work list, never recursion, so freeing a linked list of a million nodes
uses the same stack as freeing one.

### Finalizers

Ferrolua runs pending finalizers at safe points: after the current instruction finishes, before a
call returns to the host, and when a script calls `collectgarbage`. It runs them in the order their
objects reached zero. A finalizer never runs in the middle of an instruction or inside a library
function, so library code never sees Lua code run while it holds a half-updated table.

- A finalizer that stores its object somewhere brings it back; the object's count rises, and the
  finalizer doesn't run again unless `setmetatable` marks the object again.
- An error in a finalizer becomes a warning, as in Lua 5.5, with PUC-Rio Lua 5.5.1's wording,
  which M2.10 records.

### Recursion without a loop

A local function that refers to itself, as in

```lua
local function fact(n)
  if n <= 1 then return 1 end
  return n * fact(n - 1)
end
```

would hold a counted reference to itself through its upvalue. The compiler sees that `fact` is
assigned only once, by its own definition, so every use of `fact` inside the body is the function
that is running. It compiles those uses to "the running function" and creates no upvalue, so no
loop forms. A local function that is assigned anywhere else keeps an ordinary upvalue.

## Loops

Values that refer to each other in a loop keep each other's counts above zero, so they stay in
memory until the state is dropped. Common sources:

- `Class.__index = Class`, the usual class pattern without Ferrolua's class syntax;
- trees whose children point to their parents, and doubly linked lists;
- a coroutine whose function refers to the coroutine;
- two local functions that call each other.

Ferrolua's class syntax (ADR-0007) makes no loops, the linter reports the patterns above (M7.2),
and weak tables break a loop the way they do in Lua: `setmetatable(t, { __mode = "v" })` makes
`t`'s values uncounted.

## Weak tables

A table with `__mode` holding `"k"`, `"v"`, or `"kv"` keeps its weak keys or values as uncounted
handles. When the object a weak entry names is freed, its generation changes, and the table treats
the entry as absent from then on; it removes such entries when it next resizes. As in Lua, strings,
numbers, and booleans are never removed from a weak table, so those entries stay counted.

A table with weak keys and strong values, called an ephemeron table, keeps each value counted. If a
value refers to its own key, the pair forms a loop through the table and stays until the entry is
removed or the state is dropped. PUC-Rio's collector would free that pair, so this is an intended
difference.

## Memory accounting

Every allocation the state makes, from an arena slot to a table's array part, is counted in bytes.
Each one grows its buffer with `try_reserve`, so a failed allocation returns an error instead of
aborting the process.

- Without a host limit, a failed allocation raises `not enough memory`, which `pcall` can catch, as
  in PUC-Rio Lua.
- With a host limit, an allocation that would pass it stops the script with a limit error that
  `pcall` can't catch ([`sandbox.md`](sandbox.md)).

## `collectgarbage`

| Option | Ferrolua's behavior |
| :- | :- |
| `"collect"` (the default) | Runs pending finalizers, frees nothing else, and returns `0` |
| `"count"` | Returns the bytes in use divided by 1,024, as a float |
| `"step"` | Runs pending finalizers and returns `true` |
| `"isrunning"` | Returns `false` after `"stop"`, and `true` otherwise |
| `"stop"` and `"restart"` | Set the flag that `"isrunning"` reports, and change nothing else |
| `"incremental"` and `"generational"` | Return the previous mode's name, `"incremental"` at first, and store the new one |
| `"param"` | Returns the stored value of the named parameter, and stores a new one if given |

These keep scripts that tune PUC-Rio's collector running unchanged, although tuning has no effect.

## Dropping a state

Dropping a state runs every pending finalizer, then the finalizers of every object still marked,
in the reverse of the order they were marked, as `lua_close` does. It then drops the arenas, which
frees every value, loops included.

## Invariants

1. Once a state is dropped, every byte it allocated is freed.
2. An object outside any loop is freed, or its finalizer queued, as soon as its count reaches zero.
3. No operation reads an object through a stale handle; a stale handle returns an internal error.
4. Freeing any structure uses a bounded amount of stack, whatever its depth.
5. A state's counted bytes never pass its host's memory limit.
6. A self-recursive local function makes no loop.
7. A finalizer runs at most once each time its object is marked.
8. `ferrolua-vm` contains no `unsafe` code.

## Tests

- Invariant 1: a test allocator in `ferrolua-vm`'s integration tests counts bytes and checks they
  return to zero after the state is dropped, for every conformance script.
- Invariants 2 and 6: tests read the heap's object count before and after creating and dropping
  values, including 100,000 recursive local functions.
- Invariant 3: a unit test frees a slot, reuses it, and reads through the old handle, expecting the
  internal error.
- Invariant 4: a test builds and drops a linked list 10,000,000 nodes long, which would overflow
  the test thread's stack if freeing recursed.
- Invariant 5: hostile-input tests allocate past limits of 0 bytes, 1 byte, and 1 MB, and expect the
  limit error.
- Invariant 7: `tests/extensions/gc/` covers resurrection, re-marking, errors in finalizers, and the
  order at state drop.
- Invariant 8: the workspace lint `unsafe_code = "forbid"` applies to `ferrolua-vm`.
