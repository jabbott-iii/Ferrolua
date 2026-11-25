# Subsystem: `lunaferra`

The `lunaferra` crate is the one a host adds to embed Lua. It creates states with chosen libraries,
limits, and JIT mode; loads and runs chunks; moves values between Rust and Lua; registers Rust
functions and Rust types for scripts to use; and exposes the debugger interface. It wires the
compiler, the VM, and the standard library together, and the JIT too when its `jit` feature is on,
which it is not by default (ADR-0022: the JIT is an optional feature of `lunaferra`, off by
default). It adds no third-party crate to the host's build. Its API holds Rust's guarantees: a
value can't outlive its state or be used with another state. A minimal API lands in M2.14, and the
full one in M3.1.

## Why handles with no lifetime, checked against their state

Every Lua value a host holds is a reference into one state's heap, counted like any other
([`memory.md`](memory.md)). A handle has no lifetime parameter (ADR-0034: handles with no
lifetime, checked against their state), so a host stores values and callbacks in its own types,
such as an entity's `on_hit` handler, as it would any other field. Each handle keeps a weak link to
its state, built on the standard library's `Rc` and `Weak`. Using a handle after its state is
dropped returns `Error::StateDropped`, and using it with another state returns an error, so a
mistake is caught when the handle is used instead of when the host compiles. The registry stays,
for hosts that want keys.

A handle kept inside a userdata's Rust data is a reference the loop collector can't see, so a loop
through it lives until the state is dropped. A userdata holds Lua values in its user values
instead, which the collector sees.

## The API

The sketch below shows the shape; M3.1 settles the names, and the crate's documentation on docs.rs
becomes the reference.

```rust
use lunaferra::{Gc, JitMode, Libraries, Limits, Lua, Options, Value};

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
- `Value` and typed handles such as `Table`, `Function`, `List`, and `Thread` count their
  references, and release them when dropped; dropping one after its state is gone does nothing.
- Conversions between Rust and Lua values go through `IntoLua` and `FromLua` traits, implemented
  for numbers, strings, booleans, `Option`, `Vec`, `HashSet`, `HashMap`, and tuples for multiple
  values. A `Vec` converts to and from a list and a `HashSet` to and from a set, with `v[i]` as the
  list's `xs[i]`, since both count from 0, and a `HashMap` to and from a table (ADR-0025: lists,
  tuples, and sets beside tables; ADR-0026). `FromLua` also reads a `Vec` from a table's
  keys 0 to `#t - 1`. None converts between strings and numbers.
- `UserData` lets a Rust type become userdata with methods, fields, and metamethods.
- An `Api` builder describes the functions, modules, and userdata types a host gives scripts, each
  once, with its doc comment and parameter names (ADR-0035: the host's API described once). The
  types come from the Rust signatures through a `TypeInfo` trait, which every `FromLua` and
  `IntoLua` type implements: `String` is `string`, `Vec<T>` is `list<T>`, and `Option<T>` is `T?`.
  `lua.install(&api)` registers the bindings, and `api.declarations()` writes the declaration file
  that the type checker and the language server read (M3.21, M6.12). Strict mode and the type
  checker take the host's globals from the names an installed `Api` defines. `create_function`
  stays, for functions a host makes on the fly.
- `Lua::add_module(name, value)` and `Lua::add_source_module(name, source)` add modules that
  `require` finds in every state, even without `package`, and that load once per state (ADR-0036:
  modules a host adds for `require`; M3.23).
- `Packages` holds the packages of one lock file, read through a loader trait the host implements:
  from a folder, or from sources compiled into the host's binary. `Lua::add_packages(packages)`
  installs them, and `ChunkOptions::package` names the package a chunk the host loads belongs to.
  `require` resolves each name in the namespaces of the package that holds the calling function,
  after the host's modules (ADR-0038, proposed: packages and modules, as Cargo has; M14.5), as
  [`package.md`](package.md#how-a-state-uses-the-graph) describes.
- A Rust function returns `Yield(values)` to suspend the coroutine that called it, and
  `Lua::spawn(function, args)` starts a `Task` that the host runs with `Task::resume`, which gives
  `Poll::Yielded`, `Poll::Done`, `Poll::Failed`, or, for a task spawned with `pause_on_limit`,
  `Poll::OutOfBudget` when a call's instruction budget runs out (ADR-0037: scripts that wait on the
  host; M3.22). Dropping a task closes its coroutine.
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
  (ADR-0028: the targets, host services, math, and tests of the six platforms).
- `Options::clock` gives a state its wall clock and its processor clock, for `os.time`, `os.clock`,
  and `os.date`. Where the standard library has a clock, the default uses it. On
  `wasm32-unknown-unknown`, which has none, those functions are absent without a host clock, and a
  state given no `hash_seed` uses a fixed seed, so a web host passes one from the browser's
  `crypto.getRandomValues` (ADR-0028).
- The crate builds for every target in [`03-platforms.md`](../03-platforms.md#targets) (ADR-0027:
  six platforms).
- `Options::default()` sets 256 MB of memory, 1,000,000,000 instructions for each call, and 16 MB
  of source for one chunk, and `Options::trusted()` gives every library and no limits, for a host
  that runs only its own scripts (ADR-0030: finite limits by default; M3.20). `Limits::none()`
  turns the limits off and leaves the libraries as they are.
- A runtime error carries the failing expression's span and, where Lunaferra can work one out, a
  hint, beside its unchanged message (ADR-0029: runtime errors with their source line, a caret, and
  a hint; M2.20).
- `Error` covers syntax errors, runtime errors with their Lua value, limit errors, exit requests
  from `os.exit`, a JIT mode asked for without the `jit` feature or on a target without a JIT, and
  internal errors.

A host function that makes a script wait, and the task that runs it, look like this:

```rust
use lunaferra::{Api, Lua, Options, Poll, Yield};

let api = Api::new()
    .function("wait", |seconds: f64| Yield((seconds,)))
    .doc("Pauses the script until the host resumes it.")
    .params(["seconds"]);
let lua = Lua::new(Options::default())?;
lua.install(&api)?;
let task = lua.spawn(lua.load("wait(1.0) print('later')").into_function()?, ())?;
assert!(matches!(task.resume(())?, Poll::Yielded(_)));   // the script waits
assert!(matches!(task.resume(())?, Poll::Done(_)));      // the host resumed it
```

## Open questions

- Whether `Lua` implements `Send`, so a host can move a state to another thread between calls;
  it requires every registered function to be `Send` too. M3.1 decides.

## Invariants

1. Using a value after its state is dropped returns `Error::StateDropped`, and never reads freed
   memory.
2. Using a value with a state other than its own returns an error.
3. No API call panics on any input.
4. A host that adds only `lunaferra` builds no third-party crate.
5. Without the `jit` feature, a build of `lunaferra` contains no `lunaferra-jit`, and so no
   Lunaferra code with `unsafe`.

## Tests

- Invariant 1: an integration test keeps a value, a table, a function, and a task, drops their
  state, and checks that each use returns `Error::StateDropped` and each drop does nothing.
- Invariant 2: an integration test with two states.
- Invariant 3: the `api` fuzz target calls API functions with generated values and scripts.
- Invariant 4: the `cargo tree` check in [`../04-quality.md`](../04-quality.md#automated-checks).
- Invariant 5: `cargo tree -p lunaferra --edges normal --prefix none` lists no `lunaferra-jit`, a
  check that M5.4 adds to the automated checks.
