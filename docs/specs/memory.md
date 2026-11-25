# Subsystem: `memory`

This spec is the contract for how Ferrolua stores and frees Lua values. Every value is reference
counted and freed when its count reaches zero. A deterministic loop collector frees the values that
only reference loops keep alive, at points the program itself fixes (ADR-0018: reference counting
plus a loop collector run by allocation count and on request). Values live in per-state arenas and
are reached through generational handles, so the heap is safe Rust and a counting bug can't read
freed memory (ADR-0011, proposed). Bytes are counted by fixed charges, so collections fall at the
same points on every target, and each collection's finalizers run in the call that ran it
(ADR-0021, proposed: instructions are limited per call, a collection's finalizers included, and
bytes are counted alike on every target). `ferrolua-vm` implements it: the counted heap in M2.2,
and the loop collector in M2.17. `ferrolua-jit` and `ferrolua-stdlib` follow its rules.

## Why reference counting with a loop collector

A tracing garbage collector, such as PUC-Rio Lua's, finds every value still reachable from the
program and frees the rest, so it frees loops, but nothing is freed until it runs, and the VM must
tell it about every store. Reference counting frees each value at a known point, the way Rust's
`Rc` does, but can't free a loop of values that refer to each other. Ferrolua uses both: counting
frees most values at once, and a collector that looks only at suspected loops frees the rest.

Both run at points the program fixes, never on a timer or another thread. The same run therefore
frees the same values in the same order every time, which tests, replays, and games kept in
lockstep across machines depend on. The cost is a count update on every copy of a reference, and a
pause whenever the loop collector runs.

## Objects and arenas

Each kind of heap object has an arena: a growable array of slots plus a list of free slots.

| Arena | Holds | Can be in a loop |
| :- | :- | :- |
| Strings | Immutable byte strings, short ones interned (see [`vm.md`](vm.md)) | No |
| Prototypes | The compiled code of a function, shared by its closures | No |
| Tables | Array part, hash part, metatable, and weak mode | Yes |
| Closures | A Lua function: its prototype and upvalue cells | Yes |
| Native functions | A Rust function the host or the standard library registered, with its captured values | Yes |
| Upvalue cells | A captured local after its frame returns | Yes |
| Userdata | A host's Rust value, with a metatable and user values | Yes |
| Threads | A coroutine's stack and frames | Yes |
| Classes and instances | A class's methods and field layout, and an instance's fields (ADR-0007) | Yes |

```rust
struct Slot<T> {
    generation: u32,
    count: u32,
    candidate: bool,   // recorded for the loop collector
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
internal error, never another object's data. The free list hands slots out last in, first out, so
the same sequence of frees and allocations always gives the same slots.

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

## Object IDs

Lua code never sees a handle or an address. Wherever PUC-Rio would show an object's address,
Ferrolua shows the object's ID instead (ADR-0019: objects print as IDs, never addresses).

- An object has no ID until `tostring`, `string.format("%p")`, `debug.upvalueid`, or the debugger
  asks for one. Then the state's counter, which starts at 1, gives it the next number.
- A side table maps the object's handle to its ID, and a flag in the slot says the object has an
  entry, so freeing the object removes the entry.
- The counter only goes up, so an ID is never reused in its state, even after its object is freed.
  The counter is 64 bits wide, so it can't run out.
- IDs depend only on the order of requests, so they repeat from run to run with the rest of the
  program (see "Determinism" below).

## Counting

Every place that holds a `Value` holds a counted reference:

- each register and stack slot;
- each table entry, key and value, except the weak ones;
- each upvalue cell, closure, and metatable link;
- the registry, the string metatable, and the loaded-module table;
- each value the host or a native function holds through the embedding API
  ([`ferrolua.md`](ferrolua.md)).

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
call returns to the host, and when a script calls `collectgarbage`. Objects freed by counting are
finalized in the order they reached zero, and objects freed by one loop collection in the reverse
of the order they were marked, as PUC-Rio does. A finalizer never runs in the middle of an
instruction or inside a library function, so library code never sees Lua code run while it holds a
half-updated table.

Each finalizer runs in the call from the host that queued it, and counts against that call's
instruction limit ([`sandbox.md`](sandbox.md#limits); ADR-0021):

- An object freed by counting is finalized at the next safe point of the call that freed it.
- The objects one loop collection queues are finalized in the call that ran the collection,
  whether `Lua::collect`, `collectgarbage`, the schedule, or the run before a memory error started
  it. If a limit stops that call first, the rest stay queued in their order and run at the start of
  the next collection, before its own, and never at another call's safe point.

Two more rules hold for every finalizer:

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

would hold a counted reference to itself through its upvalue, a loop the collector would have to
find. The compiler sees that `fact` is assigned only once, by its own definition, so every use of
`fact` inside the body is the function that is running. It compiles those uses to "the running
function" and creates no upvalue, so no loop forms and the closure is freed by counting alone. A
local function that is assigned anywhere else keeps an ordinary upvalue.

## The loop collector

Values that refer to each other in a loop keep each other's counts above zero after the program
drops its last reference to the loop. Common sources are `Class.__index = Class` without Ferrolua's
class syntax, trees whose children point to their parents, doubly linked lists, a coroutine whose
function refers to the coroutine, and local functions that call each other. The loop collector
frees them by trial deletion, the synchronous algorithm of Bacon and Rajan's "Concurrent Cycle
Collection in Reference Counted Systems" (2001).

### Candidates

A loop becomes unreachable at the moment some reference into it is dropped and its target's count
stays above zero. So whenever a decrement leaves a count above zero, on an object of a kind that
can be in a loop, the VM records that object as a candidate, once, in the order the decrements
happen. A candidate whose count later reaches zero is freed by counting as usual, and the collector
skips its stale entry.

### Trial deletion

A collection works on the candidates and every object they reach, in three phases. It keeps trial
counts in a table of its own, so the real counts never change during a collection.

1. **Subtract:** starting from each candidate, in the order recorded, the collector visits every
   object it reaches. Each visit sets the object's trial count to its real count, minus one for each
   reference to it from another visited object. Strings and prototypes are never visited, because
   they can't be in a loop.
2. **Restore:** an object whose trial count is still above zero is referenced from outside the
   visited group: a register, the registry, the host, or a value that wasn't visited. It survives,
   and so does everything it reaches.
3. **Free:** every visited object that didn't survive is reachable only through loops.
   - Objects marked for finalization, and everything they reach, are kept and queued for their
     finalizers, following the rules of §2.5.4 for objects being finalized in weak tables.
   - The rest are freed in ascending order of arena and slot.

The collector needs no list of roots, because every place that holds a value holds a count. A value
the host holds through the embedding API keeps its count, so the collector never frees it.

### Schedule

The collector runs only at safe points, and only in these cases:

- **By allocation count:** when the bytes the state has allocated since the last collection pass
  `pause` percent of the bytes in use right after it. `pause` is 100 by default, so the collector
  runs once the program has allocated as much again as it held. The host sets it in
  `ferrolua::Options`, and a script with `collectgarbage("param", "pause", n)`.
- **On request:** when a script calls `collectgarbage()` or `collectgarbage("step")`, or the host
  calls `Lua::collect`.
- **Before a memory error:** when an allocation would fail or pass the host's limit, the VM runs
  one collection first, inside the allocation. Every value the running instruction holds is
  counted, so none is freed; the finalizers this collection queues wait for the next safe point.

`collectgarbage("stop")` turns off the runs by allocation count, and `collectgarbage("restart")`
turns them back on. A collection with no candidates does nothing.

### Determinism

Given the same script, the same inputs, the same calls from the host, the same options, and the same
hash seed, on any target, every collection runs after the same instruction, and every value is
freed and finalized in the same order.

- The threshold counts only the bytes the state allocates for Lua values. Machine code from the JIT
  and the compiler's working memory don't count, so a script collects at the same points interpreted
  and compiled.
- Bytes are counted by fixed charges, never by the target or the allocator (see "Memory accounting"
  below), so the schedule, the host's memory limit, and `collectgarbage("count")` agree on every
  target. Only an allocation that the system itself refuses, below the host's limit, can fail at a
  point that differs between machines.
- Candidates are processed in the order they were recorded, collected objects are freed in slot
  order, and slots are reused last in, first out, so no order depends on an address or a hash.
- The string hash seed is random by default, so the order `pairs` visits keys changes between runs,
  as it does in PUC-Rio Lua 5.5. A host that needs runs to repeat exactly fixes the seed with
  `Options::hash_seed`. Tables, functions, threads, and userdata used as keys hash by their
  handles, which slots reused in a fixed order make the same in every run (ADR-0020).
- In tests, the VM can record a collection log: each collection's position in the instruction
  count, the counted bytes before and after it, and the handles it freed, in order.

## Weak tables

A table with `__mode` holding `"k"`, `"v"`, or `"kv"` keeps its weak keys or values as uncounted
handles. When the object a weak entry names is freed, its generation changes, and the table treats
the entry as absent from then on. As in Lua, strings, numbers, and booleans are never removed from
a weak table, so those entries stay counted.

A table with weak keys and strong values is an ephemeron table. For the loop collector, Ferrolua
treats each entry's value as referenced by the entry's key, not by the table. A value that refers
back to its own key therefore doesn't keep the pair alive, as §2.5.4 of the manual requires. When a
weak key is freed, its entry is removed and its value released at the next collection, or when the
table next resizes, whichever comes first.

## Memory accounting

Every allocation the state makes, from an arena slot to a table's array part, is counted in bytes.
The count is a charge this section sets, never `size_of`, `Vec::capacity`, or what the allocator
returns (ADR-0021), so the same allocations count the same bytes on every target and with every
allocator:

- An object counts its kind's charge, which covers its slot, from its allocation until it is freed.
- A buffer counts its capacity times a charge for each element: 16 bytes, the size of a `Value`, for
  each slot of a table's array part or a thread's stack, and 1 for each byte of a string.
- Ferrolua sizes each buffer by its own growth rules and reserves exactly that capacity with
  `try_reserve_exact`, so a failed allocation returns an error instead of aborting the process.
- The tables that index objects, such as the string intern table and the object-ID side table,
  charge a fixed amount for each entry.

The task that builds each kind of object or buffer, such as hash nodes, call frames, and
instructions, records its charges here. A test checks that each charge is no smaller than
`size_of` of the type it stands for. The count leaves out the allocator's own overhead and the free
slots an arena keeps for reuse. After the collection that runs before a memory error:

- without a host limit, a failed allocation raises `not enough memory`, which `pcall` can catch, as
  in PUC-Rio Lua;
- with a host limit, an allocation that would still pass it stops the script with a limit error
  that `pcall` can't catch ([`sandbox.md`](sandbox.md)).

## `collectgarbage`

| Option | Ferrolua's behavior |
| :- | :- |
| `"collect"` (the default) | Runs pending finalizers, a full loop collection, and the finalizers it queued; returns `0` |
| `"count"` | Returns the bytes in use divided by 1,024, as a float |
| `"step"` | Runs a full loop collection and its finalizers, and returns `true` |
| `"isrunning"` | Returns `false` after `"stop"`, and `true` otherwise |
| `"stop"` and `"restart"` | Turn the collections by allocation count off and on |
| `"incremental"` and `"generational"` | Return the previous mode's name, `"incremental"` at first, and store the new one; the collector has one mode |
| `"param"` | `"pause"` sets the threshold above; the other parameters are stored and returned, with no effect |

A script that calls `collectgarbage()` and then checks what was freed sees what it sees on PUC-Rio.
A script that observes freeing without calling it can differ, because counting frees values earlier
than PUC-Rio's collector does.

## Dropping a state

Dropping a state runs every pending finalizer, then the finalizers of every object still marked,
in the reverse of the order they were marked, as `lua_close` does. It then drops the arenas, which
frees every value left.

## Invariants

1. Once a state is dropped, every byte it allocated is freed.
2. An object outside any loop is freed, or its finalizer queued, as soon as its count reaches zero.
3. After `collectgarbage()`, no object remains that only loops keep alive, apart from those kept for
   their finalizers.
4. The loop collector never frees an object that a register, the registry, or the host can still
   reach.
5. No operation reads an object through a stale handle; a stale handle returns an internal error.
6. Freeing and collecting use a bounded amount of stack, whatever the structure's depth.
7. A state's counted bytes never pass its host's memory limit.
8. Two runs with the same script, inputs, host calls, options, and hash seed give the same
   collection log, interpreted and with the JIT forced on.
9. A finalizer runs at most once each time its object is marked, and the finalizers of one
   collection run in the reverse of the order their objects were marked.
10. A self-recursive local function makes no loop.
11. `ferrolua-vm` contains no `unsafe` code.
12. No two objects in a state ever get the same ID, and no ID is reused after its object is
    freed.
13. The counted bytes depend only on the allocations Ferrolua makes, so the same run gives the same
    collection log, counted bytes included, on every target.
14. Each charge is no smaller than `size_of` of the type it stands for.
15. The finalizers one collection queues run only in the call that ran it, or at the start of a
    later collection.

## Tests

- Invariant 1: a test allocator in `ferrolua-vm`'s integration tests counts bytes and checks they
  return to zero after the state is dropped, for every conformance script.
- Invariants 2 and 10: tests read the heap's object count before and after creating and dropping
  values, including 100,000 recursive local functions.
- Invariant 3: tests build loops of each kind in the arena table, drop them, call
  `collectgarbage()`, and expect the heap back at its size before; `tests/conformance/gc.lua` does
  the same through weak tables, with output recorded from PUC-Rio.
- Invariant 4: a property test builds random graphs of tables, holds random parts from registers,
  the registry, and the host, collects, and checks that exactly the unreachable objects are gone.
- Invariant 5: a unit test frees a slot, reuses it, and reads through the old handle, expecting the
  internal error.
- Invariant 6: a test builds and drops a linked list 10,000,000 nodes long, and a loop of the same
  length, which would overflow the test thread's stack if freeing or collecting recursed.
- Invariant 7: hostile-input tests allocate past limits of 0 bytes, 1 byte, and 1 MB, with and
  without garbage loops the emergency collection can free.
- Invariant 8: the conformance runner records the collection log of each script twice with a fixed
  seed, and once with the JIT forced on, and compares them; the `differential` fuzz target does the
  same on generated programs (M5.8).
- Invariant 9: `tests/extensions/gc/` covers resurrection, re-marking, errors in finalizers, the
  order within a collection, and the order at state drop.
- Invariant 11: the workspace lint `unsafe_code = "forbid"` applies to `ferrolua-vm`.
- Invariant 12: a property test creates, names, and frees objects in generated orders, and
  checks that every ID it saw is distinct.
- Invariant 13: the conformance runner compares each script's collection log with the log stored
  for it (M2.17), on every target in [`../03-platforms.md`](../03-platforms.md).
- Invariant 14: a unit test in `ferrolua-vm` compares each charge with `size_of` of its type.
- Invariant 15: `crates/ferrolua/tests/hostile_sandbox.rs` stops a collection at its instruction
  limit partway through its finalizers, and checks where the rest run
  ([`sandbox.md`](sandbox.md#tests)).
