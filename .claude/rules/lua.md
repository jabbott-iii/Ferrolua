---
paths:
  - "**/*.lua"
  - "**/*.rockspec"
  - ".luacheckrc"
  - ".luarc.json"
  - "selene.toml"
  - "stylua.toml"
  - ".stylua.toml"
  - ".busted"
---

# Lua standards

These rules cover the Lua in this repository: modules, scripts, plugins, tests, and the
configuration around them. They describe how a senior engineer writes Lua, and how to work with
the user as a pair: you write the code, and they review it and make the calls that are theirs.
Doc comments, error messages, and other text follow `writing.md` in this folder.

Apply them to code you write or edit. Don't rewrite existing code only to make it conform; fix a
passage when you are already changing it, and mention anything else you notice.

## Project details

A repository fills in this section when it adopts the rule, and the rest of the rule refers to
each value by its bold name. If a value still reads `[FILL IN: …]`, the rule isn't set up here
yet. Work out that value from the repository itself, and tell the user which values are missing
so they can fill them in.

- **Lua version:** Lua 5.5, which Ferrolua implements and extends (architecture decision records
  ADR-0001 and ADR-0004 in `docs/decisions.md`). Every `.lua` file in this repository must run the
  same on Ferrolua and on PUC-Rio Lua 5.5.1, the reference interpreter, unless it tests something
  only Ferrolua has, such as its embedding API, its language extensions, or an intended difference
  the decision log records; those files go in `tests/extensions/`
- **Host:** Ferrolua itself, which provides Lua 5.5's standard libraries and whatever an embedding
  host registers. The conformance scripts also run on PUC-Rio's standalone `lua` 5.5.1 interpreter,
  to record the output Ferrolua must match
- **Modules:** none yet. The repository's Lua is test and benchmark scripts in `tests/conformance/`,
  `tests/extensions/`, and `benches/lua/`, each run as a main chunk; none loads another with
  `require`, except the scripts M3.10 adds to `tests/conformance/require/`
- **Check commands:** the conformance run in `cargo test --workspace --locked`. Once Ferrolua's own
  formatter, linter, and type checker exist, they check every script too
- **Lint configuration:** none yet. Third-party tools such as StyLua and luacheck aren't allowed
  (see `dependencies.md`), so Ferrolua's own formatter and linter will check the scripts once they
  exist, configured as their specs in `docs/specs/` describe
- **Annotations:** LuaCATS (Lua Comment And Type System) annotations, such as `---@param` and
  `---@return`, which Ferrolua's type checker will read, because these scripts must also run on
  PUC-Rio Lua 5.5.1. Scripts in `tests/extensions/` may use Ferrolua's inline annotations instead,
  as `docs/specs/language.md` defines them. lua-language-server reads LuaCATS too, but the project
  doesn't require it
- **Errors:** raise with `error` for bugs and broken contracts, and return `nil, message` for
  failures a caller expects, as Lua 5.5's standard library does. Conformance scripts catch errors
  with `pcall` and print the message, so it is compared with PUC-Rio Lua 5.5.1's
- **Dependency policy:** `dependencies.md` in this folder
- **C boundary:** none: Ferrolua is written in Rust and has no Lua C API (`lua.h`), so C modules
  built for PUC-Rio Lua don't load in it. Hosts reach Lua through Ferrolua's Rust embedding API,
  which `rust.md` and its **Unsafe policy** cover
- **Languages:** English only. Runtime error messages keep PUC-Rio Lua 5.5.1's English wording,
  because scripts match on them
- **Untrusted input:** none in the repository's own scripts. The scripts that hosts give Ferrolua
  are untrusted, and **Untrusted input** in `rust.md` covers them
- **Sandbox:** Ferrolua runs Lua its hosts didn't write, so the VM is the sandbox. A host must pick
  each state's libraries from an allowlist. `io`, `os`, `debug`, `package`, and loading precompiled
  chunks must stay off unless the host turns them on. Limits on memory and instructions must be
  enforced inside the VM, so they also stop a long call such as `string.find`. ADR-0014, still
  proposed, sets the design, and `docs/specs/sandbox.md` gives the details; the owner accepts it
  before the limits land in M3.2
- **Performance budgets:** none agreed yet. Benchmarks in `benches/lua/` will compare Ferrolua with
  PUC-Rio Lua 5.5.1 on the same x86-64 Linux machine (M5.1), and `docs/00-overview.md` proposes the
  budgets
- **Published rocks:** none
- **Design docs:** `docs/specs/<name>.md`, as in `rust.md`, with `docs/specs/language.md` for the
  language itself; the test and benchmark scripts have none of their own
- **Test layout:** conformance scripts in `tests/conformance/`, named after the feature they cover,
  such as `goto.lua`, each with the output PUC-Rio Lua 5.5.1 printed in a `.out` file beside it;
  scripts for Ferrolua-only behavior in `tests/extensions/`, each with a `.out` file written from
  the spec section it tests. A Rust integration test runs each script on Ferrolua, interpreted and
  with the JIT forced on, and compares the output. busted isn't used, because it needs C modules
  such as LuaFileSystem, which Ferrolua can't load. Lua benchmark scripts go in `benches/lua/`

## Working as a pair

- **Read before you write.** Read the code around a change and the tests that cover it, and
  match the patterns already there unless they are the problem. Check the **Modules** to find
  where new code belongs, and the **Host** to learn what it already provides.
- **Say the plan first.** For a change bigger than a few lines, say in two or three sentences
  what will change, where, and how you will check it. Then go ahead, unless the plan includes one
  of the decisions in the next point.
- **Ask before decisions that are the user's.** Stop and ask before you:
  - add a rock or a vendored library;
  - change a public module's functions, a saved data format, or the API the code offers to the
    **Host** or to plugins;
  - use a feature that one of the versions in the **Lua version** lacks, or change that list;
  - add a global;
  - change anything at the **C boundary** or in the **Sandbox**;
  - weaken a lint, a test, or a check;
  - or build something different from what was asked.

  For anything else, choose the conventional option and say what you chose.
- **Push back with reasons.** If a requested approach is unsound, slow, or fights the codebase,
  say so before you implement it, with the reason and a concrete alternative. Once the user
  decides, follow their decision.
- **Work in small steps.** Run the spec file for the module you changed after each step; that is
  much faster than the whole suite. Run all the **Check commands** before you report done.
- **Fix the cause, not the symptom.** For a bug, first write a test that fails, then fix the bug
  and show the test passing. Don't add an `or {}` or an `if x == nil then return end` that
  quietly skips a `nil` that should never be there; find out why it is `nil`.
- **Don't silence the linter.** Never add `-- luacheck: ignore`, `-- selene: allow(…)`,
  `---@diagnostic disable`, a new entry in `globals`, or a `pcall` only to make a warning or an
  error go away. Work out what it is saying, then fix the code.
- **Explain the Lua that isn't obvious.** In your reply, say why you used a metatable, an
  `__index` chain, varargs, a coroutine, an upvalue, or an environment (`_ENV` or `setfenv`).
  Write for the audience in `writing.md`, and cover only what this change needs.
- **Keep the change focused.** No unrelated refactors, renames, or reformatting. When the
  formatter changes files you didn't otherwise touch, revert those changes. Mention problems you
  notice outside the task instead of fixing them.
- **Review your own diff** before you report, as a reviewer would. Look for a missing `local`, a
  call with `.` where `:` was meant, `#` on a table that can have holes, `..` in a loop, a
  `pcall` whose error is dropped, a debugging `print`, and missing docs or tests.
- **Report honestly.** Say what changed and why, each command you ran with its real result, what
  you didn't run and why, your assumptions, and the risks that remain. Never say a check passed
  unless you ran it in this session and saw it pass.

## Know which Lua you are writing

Every file must run on each implementation in the **Lua version**, inside the **Host**. The
versions differ more than their numbers suggest, so check a feature before you use it:

| Feature | Official Lua | LuaJIT 2.1 |
| --- | --- | --- |
| `goto` and labels | Since 5.2 | Yes |
| `load` with a mode and an environment | Since 5.2, which also replaced `setfenv` with `_ENV` | `load` with a mode and an environment, and `setfenv`; no `_ENV` |
| `table.pack` and `table.unpack` | Since 5.2, which removed the global `unpack` | Only when built with `LUAJIT_ENABLE_LUA52COMPAT`; otherwise the global `unpack` |
| `__gc` on tables | Since 5.2 | No: on userdata only |
| Integers, `//`, `math.type`, and `utf8` | Since 5.3 | No: every number is a double |
| Bitwise operators (`&`, `\|`, `~`, `<<`, `>>`) | Since 5.3 | In current builds; every build has the `bit` module |
| `<const>` and `<close>` variables | Since 5.4 | No |
| `global` declarations, and read-only `for` loop variables | Since 5.5 | No |
| `continue` and `+=` | Never | In current builds |

- These held for Lua 5.1.5, 5.4.9, and 5.5.1, and for LuaJIT 2.1 at commit `96f7b29`
  (November 2026). LuaJIT 2.1 has no release tarballs: it is a rolling release from its Git
  repository, and a host may embed a build years older than that one.
- Hosts fix the version for you. Neovim promises the Lua 5.1 interface and recommends LuaJIT,
  Redis runs Lua 5.1, OpenResty runs LuaJIT, and Pandoc's filters run Lua 5.4.
- Luau, the dialect Roblox uses, is based on Lua 5.1 and adds gradual types, `continue`, and
  compound assignment. Follow Luau's own documentation where it differs from this rule.
- If the code must run on several versions, write to what they share, and detect the rest
  instead of assuming it: `local unpack = table.unpack or unpack`.

## Variables, globals, and modules

- Declare every variable `local`. Assigning to an undeclared name creates a global, so a typo
  silently creates a new variable, and reading a misspelled name silently gives `nil`.
- Let the linter catch the globals you didn't mean. Declare the ones the **Host** provides as
  read-only in the **Lint configuration** (luacheck's `read_globals`), never as writable.
- On Lua 5.5, a `global` declaration ends implicit globals for the rest of its scope: after
  `global print`, any other undeclared name is a compile error.
- Write each module as a table it returns, and set no globals:

  ```lua
  local M = {}

  --- Returns the larger of `a` and `b`.
  function M.max(a, b)
    if a > b then
      return a
    end
    return b
  end

  return M
  ```

- Never use `module(...)`, which Lua 5.2 deprecated.
- `require` runs a module once and caches what it returns in `package.loaded`. Keep the work a
  module does while it loads to defining its functions and constants, so the order in which
  modules load doesn't matter.
- Break circular `require` chains. Lua 5.1 and LuaJIT report a loop, but Lua 5.4 and 5.5
  recurse until the stack overflows. Move the shared part into a third module, or `require` it
  inside the function that needs it.
- Leave `package.path` and `package.cpath` to the **Host** or the program's entry point; a
  library that changes them changes them for everything else.

## Tables and strings

- Use `#` only on sequences, which are tables whose keys run from 1 to n with no `nil` between.
  On a table with holes, `#` may return any border (an index whose next index is `nil`). On
  `{1, nil, 3}`, Lua 5.4 and 5.1 returned 3, and Lua 5.5 and LuaJIT returned 1. Keep an
  explicit count `n` for a list that can hold `nil`.
- Count varargs with `select("#", ...)`, which counts `nil` values, or `table.pack(...)` with its
  `n` field (Lua 5.2 and later), never `#{...}`.
- `ipairs` stops at the first `nil`. `pairs` visits keys in no defined order, so sort the keys
  when output, tests, or saved files depend on the order.
- Don't add new keys to a table while you traverse it with `pairs` or `next`. You may change or
  clear (set to `nil`) the fields that already exist.
- Test whether a table is empty with `next(t) == nil`, which also works on tables that aren't
  sequences.
- `t[1]` and `t["1"]` are different keys. From Lua 5.3 on, a float key with an integer value is
  stored as the integer: `t[1.0]` is `t[1]`.
- Build a long string by collecting pieces in a table and joining them with `table.concat`.
  Joining with `..` in a loop copies the whole string each time, so the time grows with the
  square of its length. On Lua 5.4.9, on an Intel Core i7-11800H, 80,000 pieces took 12.2 s with
  `..` and 0.01 s with `table.concat`.
- Strings are bytes. `#s` counts bytes, so `#"é"` is 2 in UTF-8. Use the `utf8` library (Lua 5.3
  and later) to count or walk code points, and never cut a UTF-8 string in the middle of one.
- Lua's patterns aren't regular expressions. They have no `|` for alternatives, and `%` escapes
  the special characters `^$()%.[]*+-?`. To search for literal text, pass `true` as the fourth
  argument of `string.find`: `string.find(s, ".", 1, true)` finds a period, not any character.
- To write a string as Lua source, use `string.format("%q", s)`, which escapes it so it reads
  back exactly.

## Numbers

- From Lua 5.3 on, numbers are integers or floats. `10 / 2` is the float `5.0`; `//` divides and
  rounds down; `math.tointeger` converts a float with an integer value. `string.format("%d",
  3.5)` raises an error in Lua 5.3 and later, but quietly truncates to 3 in Lua 5.1 and LuaJIT.
- Integer arithmetic wraps around on overflow, so `math.maxinteger + 1 == math.mininteger`.
  Check sizes and counts from input before you add or multiply them.
- In Lua 5.1 and LuaJIT, every number is a double, which holds integers exactly only up to
  2^53: `2^53 == 2^53 + 1` is true.
- Integer division and remainder by zero raise errors in Lua 5.3 and later (`1 // 0`, `1 % 0`).
  Float division gives infinity, and `1 % 0` in Lua 5.1 gives NaN (not a number).
- Don't rely on coercion between strings and numbers. `"10" + 1` is 11, but `"10" == 10` is
  false, and Lua 5.4 stopped coercing strings in bitwise operations. Convert input with
  `tonumber` where it enters the program, and check for `nil`.
- Know what `tonumber` accepts: surrounding spaces, hexadecimal (`"0x10"` is 16), and exponents
  (`"1e3"` is the float `1000.0`). Match the text with a pattern first when only digits are
  valid.
- Keep money in integers of the smallest unit, on Lua 5.3 and later, or in a decimal library;
  never in floats.

## Values and functions

- Only `nil` and `false` are false. `0` and `""` are true.
- `a and b or c` returns `c` whenever `b` is `false` or `nil`. Use an `if` statement when `b`
  can be either.
- Call methods with `:`, which passes the table as `self`. `account.deposit(10)` instead of
  `account:deposit(10)` fails with an error such as "attempt to index a nil value (local
  'self')".
- A call in the form `return f(x)` is a proper tail call, which reuses the caller's stack frame,
  so recursion in that position doesn't grow the stack.
- Prefer a table of named options to a long list of parameters, and validate it on entry.

## Errors

Follow the **Errors** convention.

- Return `nil, message` for failures a caller should expect and handle, as `io.open` does. Raise
  with `error` for bugs and broken contracts.
- Raise argument errors with level 2, `error("amount must be positive", 2)`, so the message
  points at the caller's line, not yours.
- Raise a table when callers need to tell errors apart, such as `{code = "not_found"}`, and give
  it a `__tostring` metamethod so logs stay readable.
- Catch errors with `pcall`, or with `xpcall(f, debug.traceback)` to keep the stack trace. Catch
  them at boundaries, such as a request handler, a plugin's entry point, or the frame loop, not
  around every call, and never drop the error.
- `assert(io.open(path))` raises the message `io.open` returned. That suits a script; a library
  returns the error to its caller instead.
- Lua 5.5 replaces a `nil` error value with a string: `pcall(error)` returns a string there, and
  `nil` in Lua 5.4.
- Release resources on every path. On Lua 5.4 and later, a `<close>` variable is closed when its
  scope ends, even by an error, in reverse order of declaration. Elsewhere, close the resource
  after a `pcall`, then raise the error again. Don't count on `__gc` to close files: it runs when
  the collector gets to the object, which may be never before the program exits.

## Metatables and objects

- Write a class as a table that is its own `__index`:

  ```lua
  local Account = {}
  Account.__index = Account

  function Account.new(balance)
    return setmetatable({ balance = balance }, Account)
  end

  function Account:deposit(amount)
    self.balance = self.balance + amount
  end

  return Account
  ```

- Prefer composition to inheritance, and keep any inheritance to one level, because each level
  adds an `__index` lookup and another place a field can come from.
- Inside `__index` and `__newindex`, use `rawget` and `rawset` to avoid calling the metamethod
  again.
- A read-only table made with `__newindex` stops mistakes, not attackers: `rawset` writes past
  it.
- `__gc` on a table runs only in Lua 5.2 and later; Lua 5.1 and LuaJIT run it on userdata only.
- Give a cache keyed by objects weak keys (`setmetatable(cache, { __mode = "k" })`), so the
  cache doesn't keep those objects alive.

## Coroutines and the host's loop

- Coroutines are cooperative: one runs until it yields. A loop that never yields stops
  everything else in the program.
- `coroutine.resume` returns `false` and the error instead of raising it, so check its first
  result. `coroutine.wrap` raises the error in the caller.
- Lua 5.2 and later, and LuaJIT, can yield inside a `pcall`. In Lua 5.1 the yield fails, and the
  `pcall` catches the failure, so the coroutine carries on without yielding and no error
  appears.
- On Lua 5.4 and later, call `coroutine.close` on a coroutine you won't resume, so its `<close>`
  variables are closed.
- Never block the **Host**'s loop:
  - Redis blocks the whole server while a script runs, so keep scripts short.
  - In OpenResty, use the nginx Lua API's non-blocking sockets, not a blocking library.
  - In Neovim, don't call `vim.api` functions from `vim.uv` callbacks; defer that work with
    `vim.schedule`.
  - In a game, spread long work across frames, within the **Performance budgets**.

## Performance and memory

- Measure before you optimize, on each implementation in the **Lua version** and on the hardware
  the **Performance budgets** name. Under LuaJIT, `-jv` shows which loops its compiler traced and
  where it gave up, and `-jp` starts its profiler.
- Keep allocation out of hot paths. Creating tables, closures, and strings in a loop that runs
  every frame or every request makes work for the garbage collector, which shows up as pauses.
  Reuse tables instead.
- In a measured hot loop, copy a library function into a local (`local abs = math.abs`). That
  saves a table lookup on each call: 2 × 10^7 calls took 1.57 s through `math.abs` and 1.14 s
  through a local, on Lua 5.4.9 on an Intel Core i7-11800H.
- Read memory use with `collectgarbage("count")`, in kilobytes. Tune the collector only after
  measuring, and only with the user's agreement.

## Text for users and languages

This section covers the text the program shows people, through the **Host** or a terminal. With
"English only" as the **Languages** value, the first point still applies, so the program can be
translated later.

- Keep user-facing text in the **Languages** location, and look it up by key through one helper,
  such as `t("files_deleted", { count = n })`, or through the **Host**'s system where it has one.
  Keep log and error messages for developers in English, and out of the translations.
- `string.format` can't reorder its arguments. It rejects `%1$s` with "invalid conversion", so a
  translation would have to keep the source's word order. Use named placeholders instead, and
  fill them with `gsub` and a table: `(msg:gsub("{(%w+)}", args))`. The parentheses keep only
  the string, because `gsub` also returns a count. A name missing from `args` stays in the text
  as `{name}`, where a test can catch it.
- Lua has no plural rules. Never choose a word form with `n == 1`: write a function for each
  language from the plural rules of the Unicode Common Locale Data Repository (CLDR), and test
  each one at the numbers where its form changes. Arabic's six forms need at least 0, 1, 2, 3,
  11, and 100, one number for each.
- Lua formats with the C library's locale, which the standalone interpreter leaves at `"C"`, so
  `string.format("%.2f", 1.5)` writes `1.50`. Don't call `os.setlocale` to format for a user:
  it changes the whole process, and after `os.setlocale("fr_FR.UTF-8")`, `tostring(1.5)` writes
  `1,5`. Format numbers and dates for people with the **Host**'s tools, or with code of your own.
- Strings are bytes, as "Tables and strings" says. `string.upper`, `string.lower`, and `%a` in
  patterns work one byte at a time, so they skip letters outside ASCII: `string.upper("é")`
  returns `"é"`. Count and cut translated text with the `utf8` library (Lua 5.3 and later), never
  with `#` and `string.sub`.
- Check that the **Host** shapes text. Joining Arabic and Urdu letters, and combining the letters
  and marks of Hindi and Bengali, needs a text shaper such as HarfBuzz, and not every game engine
  or terminal has one.
- Load translation files that ship with the program as modules. Load any other translation file,
  such as one that players contribute, as data (JSON, for example) or through the **Sandbox**: a
  translation written in Lua is code, as "Untrusted input" says.
- Treat translations you write as drafts, and list their keys in your report for a native
  speaker to review.

## Untrusted input

Treat every byte from outside the program as possibly malformed or hostile, especially in the
**Untrusted input** modules.

- Never run data as code. Don't read a data format by passing it to `load`, `loadstring`,
  `dofile`, or `require`; use a parser for that format.
- Refuse binary chunks (compiled bytecode). Lua doesn't check bytecode, and crafted bytecode can
  crash the interpreter. Pass the mode `"t"` to `load` (Lua 5.2 and later, and LuaJIT), so it
  accepts only source text. Lua 5.1's `loadstring` takes no mode, so reject a chunk whose first
  byte is 27 (`"\27"`), which every binary chunk starts with.
- Check sizes before you act on them: lengths before `string.rep`, counts before building
  tables, and depth before recursing. `string.rep` has no limit of its own, and one call can
  exhaust the machine's memory.
- Never let input choose a pattern, and keep patterns that can backtrack away from input text.
  On Lua 5.4.9, a pattern of `a?` repeated n times followed by n `a`s took about four times as
  long for every two characters added to n: 0.49 s at 22 and 1.94 s at 24.
- Never build a command for `os.execute` or `io.popen` from input; both pass the string to a
  shell. Use an API that takes each argument separately.
- Treat paths from input as hostile. Reject absolute paths and `..` components before you pass a
  path to `io.open`.

## Sandboxing code you didn't write

Where the **Sandbox** isn't "none", the program runs Lua it didn't ship. A sandbox written in
Lua is only as strong as the smallest thing it lets that code reach, so keep it small, and ask
the user before you change it.

- Load the code as text, with an environment of its own: `load(source, "=plugin", "t", env)` on
  Lua 5.2 and later, or `setfenv(chunk, env)` on Lua 5.1.
- Build `env` from an allowlist of the functions the code needs, never from a copy of `_G` with
  the dangerous parts removed. Never include `os`, `io`, `debug`, `package`, `require`, `load`,
  `loadstring`, `loadfile`, `dofile`, `string.dump`, `collectgarbage`, `getmetatable`, or
  `setmetatable`.
- Copy each library table you hand over, instead of sharing it, so the code can't replace a
  function the host uses. `getmetatable` is excluded because strings share one metatable whose
  `__index` is the real `string` table: given `getmetatable`, sandboxed code changed what the
  host's `("host"):upper()` returned.
- Stop runaway code with a count hook, which calls a function every n instructions:
  `debug.sethook(function() error("instruction limit reached", 0) end, "", 1e7)`. The hook runs
  only between Lua instructions, so it can't stop a long call inside a C function such as
  `string.find`: during a 1.9-second pattern match it never fired. It doesn't limit memory
  either.
- For hard limits on time and memory, run the code in a separate process with limits set by the
  operating system, or give the C host a custom allocator (the function passed to
  `lua_newstate`) that refuses memory past a budget.

## The C boundary

Where Lua meets C, the **C boundary**, memory safety is your job again.

- Keep the Lua stack balanced: compare `lua_gettop` before and after, and call `luaL_checkstack`
  before pushing more than a few values.
- Check every argument with the `luaL_check…` and `luaL_opt…` functions.
- An error from `lua_error` or `luaL_error` jumps out of the C function without returning, so
  memory from `malloc` leaks. Allocate it as userdata with a `__gc` (or `__close`) metamethod
  instead.
- A string from `lua_tostring` stays valid only while its value is on the stack; copy it if you
  need it longer.
- Hold Lua values from C with `luaL_ref` in the registry, and release them with `luaL_unref`.
- In the host, call Lua with `lua_pcall`. An error outside any protected call calls Lua's panic
  function, and the program aborts.
- LuaJIT's FFI lets Lua call C directly, with none of Lua's memory safety. Treat each
  `ffi.cdef` declaration and each pointer like C code: check lengths, keep the owning cdata (the
  Lua value that holds C data) alive while C uses it, and attach `ffi.gc` finalizers to memory C
  allocated.

## Documentation

Write docs in the form the **Annotations** value names, and don't mix forms in one project.

- With lua-language-server, document with LuaCATS annotations, which its checks also read:

  ```lua
  --- Moves money between two accounts of the same currency.
  ---@param from Account The account to debit.
  ---@param to Account The account to credit.
  ---@param cents integer The amount, in cents; must be positive.
  ---@return Receipt|nil receipt The receipt, or nil if the transfer failed.
  ---@return string? err Why the transfer failed.
  function M.transfer(from, to, cents)
  ```

- With LDoc, use its tags, such as `@param`, `@tparam`, and `@treturn`, and build the docs with
  its configuration.
- Document every public function: what it does, each parameter with its unit and valid range,
  what it returns (including the `nil, message` case), the errors it raises, and its effects on
  files, globals, or the **Host**.
- Comments explain why, not what. Delete commented-out code instead of leaving it.
- Keep each module's design in the **Design docs**, and update them in the same change as the
  code.

### Rocks

Each of the **Published rocks** needs a README and a rockspec of its own.

- The README says what the rock does, how to install it (`luarocks install <name>`), which Lua
  versions it supports, and gives a short example that a test runs, and its license.
- The rockspec's `version` is the rock's version plus a rockspec revision, `"1.2.0-1"`. Raise the
  revision when only the rockspec changes:

  ```lua
  rockspec_format = "3.0"
  package = "money"
  version = "1.2.0-1"
  source = {
    url = "git+https://github.com/example/money.git",
    tag = "v1.2.0",
  }
  description = {
    summary = "Exact money arithmetic in integer cents.",
    homepage = "https://github.com/example/money",
    license = "MIT",
  }
  dependencies = {
    "lua >= 5.3",
  }
  build = {
    type = "builtin",
    modules = {
      money = "src/money.lua",
    },
  }
  ```

## Tests

- Follow the **Test layout**. With busted, group specs with `describe` and `it`, compare tables
  with `assert.are.same`, which compares contents, and test errors with `assert.has_error`.
- Run the tests on every implementation in the **Lua version**. hererocks installs a chosen Lua
  or LuaJIT, with LuaRocks, into a local folder, so each can be tested side by side.
- Test the failure paths: the `nil, message` returns, the errors raised, and input at each limit.
- Measure coverage with LuaCov (`busted --coverage`), and read the lines it reports as never run.
- Make tests repeatable. Lua 5.4 seeds `math.random` differently on every run, while Lua 5.1
  repeats the same sequence, so call `math.randomseed` with a fixed seed in tests. Pass the clock
  in rather than reading `os.time` inside the code under test.
- Keep logic in plain modules that take the **Host**'s API as an argument, so most tests run
  without the host. Test the rest in the host's own test setup.

## Style, lints, and the toolchain

- Format with the formatter in the **Lint configuration**, such as StyLua (`stylua --check`
  reports files that need formatting without changing them), set to the syntax of the
  **Lua version**.
- Lint with luacheck, from the lunarmodules fork (`std` such as `"lua54"` or `"luajit"`, with
  `globals` and `read_globals`), or with selene (`std` such as `"lua51"` or `"roblox"` in
  `selene.toml`).
- Check types with lua-language-server where the project uses LuaCATS:
  `lua-language-server --check=<path>` writes its results to its log folder, at the level
  `--checklevel` sets.
- Treat lint findings as design feedback. When a suppression is right, put it on the one line it
  covers, with the reason in a comment above it:

  ```lua
  -- The host always passes `context`, which this handler doesn't need.
  local function on_event(event, context) -- luacheck: ignore 212
  ```
- Name locals and functions in `snake_case`, classes in `PascalCase`, and constants in
  `UPPER_CASE`, unless the codebase already does otherwise. Name a value you mean to ignore `_`.
- Don't leave `print` calls or debugging output behind. Log through what the **Host** provides.

## Dependencies

- Follow the **Dependency policy**. Before you suggest a rock, check that it is maintained,
  supports every implementation in the **Lua version**, and has a license the project allows.
- A rock with C code needs a compiler on every platform the project builds for, and it is part
  of the **C boundary**. Prefer pure Lua where it does the job.
- Pin versions. LuaRocks writes `luarocks.lock` when you pass `--pin` to `luarocks build` or
  `luarocks make`.
- Install rocks into a project tree (`luarocks init`) or a user tree, never system-wide with
  `sudo`.
- Keep vendored libraries in one folder, each with its version and source written down, and
  never edit them in place.

## Before finishing

1. Run every **Check commands** command on every implementation in the **Lua version**, and read
   its output. Fix every warning, not only the errors.
2. Check that the formatter and linter are clean, and that you added no globals.
3. If you changed public functions, annotations, or a README, rebuild the docs or run the type
   check, and run the tests that the README's examples come from.
4. If you changed a rockspec, run `luarocks make` with it in a clean tree.
5. If you touched the **C boundary** or the **Sandbox**, name each change in your report and say
   why it is still safe.
6. Review your diff as described in "Working as a pair", then report what you ran, what you
   didn't, and what remains.
