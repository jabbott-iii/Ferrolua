# Subsystem: `ferrolua`

The `ferrolua` crate is the one a host adds to embed Lua. It creates states with chosen libraries,
limits, and JIT mode; loads and runs chunks; moves values between Rust and Lua; registers Rust
functions and Rust types for scripts to use; and exposes the debugger interface. It wires the
compiler, the VM, and the standard library together, and the JIT too when its `jit` feature is on,
which it is not by default (ADR-0022: the JIT is an optional feature of `ferrolua`, off by
default). It adds no third-party crate to the host's build. Its API holds Rust's guarantees: a
value can't outlive its state or be used with another state. A minimal API lands in M2.14, and the
full one in M3.1.

## Why handles tied to the state by lifetime

Every Lua value a host holds is a reference into one state's heap, counted like any other
([`memory.md`](memory.md)). Tying each handle to its state with a Rust lifetime makes using a value
after its state is dropped a compile error, and using it with the wrong state a checked error.
Hosts that need to keep a value across calls without a lifetime store it in the state's registry
and keep the registry key. The cost is lifetimes in the host's code.

## The API

The sketch below shows the shape; M3.1 settles the names, and the crate's documentation on docs.rs
becomes the reference.

```rust
use ferrolua::{Gc, JitMode, Libraries, Limits, Lua, Options, Value};

let lua = Lua::new(Options {
    libraries: Libraries::SAFE,
    limits: Limits { memory: Some(64 << 20), instructions: Some(10_000_000), ..Limits::default() },
    jit: JitMode::On,                         // needs the `jit` feature
    gc: Gc { pause: 100, automatic: true },   // the loop collector's schedule
    hash_seed: None,                          // Some(seed) makes runs repeat exactly
    clock: None,                              // the host's clock, needed on the web
    ..Options::default()
})?;

lua.globals().set("greet", lua.create_function(|_lua, name: String| {
    Ok(format!("hello, {name}"))
})?)?;

let result: String = lua.load("return greet('world')").name("example").eval()?;
assert_eq!(result, "hello, world");
```

- `Lua::load` returns a chunk builder with `name`, `mode` (text or binary), `strict`, and `env`,
  then `exec`, `eval`, or `into_function`.
- `Value<'lua>` and typed handles such as `Table<'lua>` and `Function<'lua>` count their
  references, and release them when dropped.
- Conversions between Rust and Lua values go through `IntoLua` and `FromLua` traits, implemented
  for numbers, strings, booleans, `Option`, `Vec`, `HashSet`, `HashMap`, and tuples for multiple
  values. A `Vec` converts to and from a list and a `HashSet` to and from a set, with `v[i]` as the
  list's `xs[i]`, since both count from 0, and a `HashMap` to and from a table (ADR-0025: lists,
  tuples, and sets beside tables; ADR-0026). `FromLua` also reads a `Vec` from a table's
  keys 0 to `#t - 1`. None converts between strings and numbers.
- `UserData` lets a Rust type become userdata with methods, fields, and metamethods.
- `Lua::collect` runs the loop collector and the finalizers it queues, at a point the host
  chooses ([`memory.md`](memory.md#schedule)). It is a call like any other: its finalizers count
  against the instruction limit, and if the limit stops them, it returns the limit error and the
  rest run first in the next collection (ADR-0021: instructions are limited per call, a
  collection's finalizers included, and bytes are counted alike on every target).
- `Lua::set_limits` changes a state's limits between calls. The instruction limit applies to each
  call from the host on its own ([`sandbox.md`](sandbox.md#limits)).
- `JitMode` and `Options::jit` exist in both builds, so turning the `jit` feature on changes no
  host's code. Without the feature, the default is `JitMode::Off`, and `Lua::new` refuses
  `JitMode::On` and `JitMode::Forced` with an error that names the feature
  ([`jit.md`](jit.md#modes)). On iOS and the web, which have no JIT, the same holds with the
  feature on, and the error names the target. Where the operating system refuses executable
  memory, a state in `JitMode::On` runs interpreted, and `Lua::jit_running` tells the host
  (ADR-0028, proposed: the targets, host services, math, and tests of the six platforms).
- `Options::clock` gives a state its wall clock and its processor clock, for `os.time`, `os.clock`,
  and `os.date`. Where the standard library has a clock, the default uses it. On
  `wasm32-unknown-unknown`, which has none, those functions are absent without a host clock, and a
  state given no `hash_seed` uses a fixed seed, so a web host passes one from the browser's
  `crypto.getRandomValues` (ADR-0028, proposed).
- The crate builds for every target in [`03-platforms.md`](../03-platforms.md#targets) (ADR-0027:
  six platforms).
- `Error` covers syntax errors, runtime errors with their Lua value, limit errors, exit requests
  from `os.exit`, a JIT mode asked for without the `jit` feature or on a target without a JIT, and
  internal errors.

## Open questions

- Whether `Lua` implements `Send`, so a host can move a state to another thread between calls;
  it requires every registered function to be `Send` too. M3.1 decides.
- How a host lists the globals it registers, so strict mode and the type checker know them: a call
  on `Lua`, or the same `globals` list `.ferrolua.lua` uses. M4.6 decides.

## Invariants

1. No sequence of API calls lets a value be used after its state is dropped; it doesn't compile.
2. Using a value with a state other than its own returns an error.
3. No API call panics on any input.
4. A host that adds only `ferrolua` builds no third-party crate.
5. Without the `jit` feature, a build of `ferrolua` contains no `ferrolua-jit`, and so no Ferrolua
   code with `unsafe`.

## Tests

- Invariant 1: compile-fail examples in the doc comments, marked `compile_fail`, which `cargo test`
  checks.
- Invariant 2: an integration test with two states.
- Invariant 3: the `api` fuzz target calls API functions with generated values and scripts.
- Invariant 4: the `cargo tree` check in [`../04-quality.md`](../04-quality.md#automated-checks).
- Invariant 5: `cargo tree -p ferrolua --edges normal --prefix none` lists no `ferrolua-jit`, a
  check that M5.4 adds to the automated checks.
