# Subsystem: `ferrolua`

The `ferrolua` crate is the one a host adds to embed Lua. It creates states with chosen libraries,
limits, and JIT mode; loads and runs chunks; moves values between Rust and Lua; registers Rust
functions and Rust types for scripts to use; and exposes the debugger interface. It wires the
compiler, the VM, the standard library, and the JIT together, and adds no third-party crate to the
host's build. Its API holds Rust's guarantees: a value can't outlive its state or be used with
another state. A minimal API lands in M2.14, and the full one in M3.1.

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
    jit: JitMode::On,
    gc: Gc { pause: 100, automatic: true },   // the loop collector's schedule
    hash_seed: None,                          // Some(seed) makes runs repeat exactly
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
  for numbers, strings, booleans, `Option`, `Vec`, `HashMap`, and tuples for multiple values.
- `UserData` lets a Rust type become userdata with methods, fields, and metamethods.
- `Lua::collect` runs the loop collector and the finalizers it queues, at a point the host
  chooses ([`memory.md`](memory.md#schedule)).
- `Error` covers syntax errors, runtime errors with their Lua value, limit errors, exit requests
  from `os.exit`, and internal errors.

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

## Tests

- Invariant 1: compile-fail examples in the doc comments, marked `compile_fail`, which `cargo test`
  checks.
- Invariant 2: an integration test with two states.
- Invariant 3: the `api` fuzz target calls API functions with generated values and scripts.
- Invariant 4: the `cargo tree` check in [`../04-quality.md`](../04-quality.md#automated-checks).
