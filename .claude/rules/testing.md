# Testing

These rules cover the tests in this repository: unit, integration, end-to-end, property, fuzz,
performance, and security tests, the fixtures and saved outputs they read, and the coverage and
continuous integration (CI) settings that run them. Tests guard three things: that the software
works, that it stays within its performance budgets, and that it stays secure. A test guards them
only while it stays strict, so the first rule is that no test is ever weakened to make it pass.

The language rules' **Test layout** and **Check commands** still say where tests live and which
commands run them. Test names, messages, and reports follow `writing.md` in this folder. Where
`dependencies.md` is adopted, test tools are dependencies too, and it decides which ones the
project may use.

Apply these rules to code and tests you write or change. Don't rewrite existing tests only to
make them conform. When tests near your change are weaker than this rule asks, strengthen them as
part of the change, and mention any others you notice.

## Project details

A repository fills in this section when it adopts the rule, and the rest of the rule refers to
each value by its bold name. If a value still reads `[FILL IN: …]`, the rule isn't set up here
yet. Work out that value from the repository itself, and tell the user which values are missing
so they can fill them in. Until **Coverage floor** is filled in, treat it as "100% of lines and
branches".

- **Test commands:** `cargo test --workspace --locked`. It runs the Rust tests, the conformance and
  extension scripts, and every saved fuzz input
- **Coverage command:** `cargo xtask coverage`, once M0.3 builds it. A std-only `xtask` crate in the
  workspace builds the tests with `-C instrument-coverage`, merges the profiles with
  `llvm-profdata` from the `llvm-tools` rustup component, and fails when `llvm-cov` reports less
  than the **Coverage floor**
- **Coverage floor:** 100% of lines and regions. Stable Rust can't measure branches, so regions
  stand in: each `if` and `match` arm and each operand of `&&` and `||` is a region of its own, but
  a region doesn't show whether a condition was both true and false
- **Coverage exclusions:** none
- **Mutation command:** none: cargo-mutants is third party, and Ferrolua has no mutation tool of its
  own yet
- **Performance budgets:** none agreed yet. `docs/00-overview.md` proposes them, measured against
  PUC-Rio Lua 5.5.1 on the same x86-64 Linux machine by the benchmarks in each crate's `benches/`
  and in `benches/lua/`, where PUC-Rio runs each script's Lua 5.5 twin, and M5.9 makes each agreed
  budget a check that fails the build
- **Security checks:** hostile-input tests for the lexer, parser, compiler, bytecode loader, and
  sandbox limits in each crate's `tests/`; and fuzz targets for every parser of outside data, in
  `fuzz/`, run by Ferrolua's own std-only fuzzer from a fixed seed. Every input that ever crashed a
  target is saved in `fuzz/regressions/` and replayed by `cargo test`. How long each target runs in
  CI is set when the CI workflow lands. No dependency audit runs, because Ferrolua has no
  dependencies
- **CI:** none yet: Ferrolua has no GitHub Actions workflows. The first, `.github/workflows/ci.yml`,
  will run the **Check commands**, the **Test commands**, and the **Coverage command** on x86-64
  Linux, and from M3.18 on the tests on every target in `docs/03-platforms.md`, with coverage on
  Linux, Windows, and macOS, and every job required before a merge to `main`

## A failing test means the code is wrong

When a test fails, assume the code is wrong until the evidence shows otherwise. Find the cause,
and fix the code. Never change a test, its data, or its settings to make it pass. In particular,
never:

- change an expected value to whatever the code now returns
- delete a test, a test case, or an assertion
- loosen an assertion: an exact value turned into "not empty" or "is ok", an exact error turned
  into any error, or a tolerance widened
- skip a test or mark it as expected to fail, such as `#[ignore]` in Rust, `@pytest.mark.skip`
  or `xfail` in Python, or `it.skip` in JavaScript
- retry a failing test automatically, or add a sleep or a longer timeout to hide a race
- mock or stub the code under test, or catch and discard the error the test should see
- make the code behave differently under test, such as a branch on `cfg!(test)` or on an
  environment variable the tests set
- accept new snapshots or golden files (saved outputs that a test compares against) without
  reading each difference
- lower the **Coverage floor**, add to the **Coverage exclusions**, or raise a limit in the
  **Performance budgets**
- turn off a check in the **CI**, make it optional, or let it fail without failing the build

A test can be wrong: it checks behavior that the user has since changed on purpose, or it was
wrong when it was written. If you think a test is wrong, stop and tell the user which test it is,
what it expects, why you think the expectation is wrong, and what shows it, such as the
specification, an issue, or the user's request. Change the test only after the user agrees, and
give the reason in the commit message.

When the user asks for a change in behavior, update the tests that describe the old behavior in
the same change. List each changed test in your report, with what it now expects.

A test that sometimes passes and sometimes fails (a flaky test) has a bug, in the test or in the
code. Races and dependence on test order often show up this way first, so find the cause instead
of rerunning the test until it passes.

## Cover every behavior

- Aim for every line and every branch. Measure branch coverage, not only line coverage: a line
  such as `if a && b` counts as covered after one test, but each condition can go two ways, and
  each way needs a test.
- Coverage shows only that code ran, not that a test checked what it did, so every test asserts
  on its result. Mutation testing finds the gaps: a tool changes the code in small ways, such as
  turning `<` into `<=`, and checks that some test fails for each change. Each change that no test
  catches, which cargo-mutants calls "missed" and mutmut calls "surviving", needs a new test, or
  a reason in your report why the change can't alter the behavior.
- Test each outcome a function can have: each kind of success, each error it returns, and each
  panic or exception it documents.
- Test the edges: zero, one, and many; empty and full; the smallest and largest values; one past
  each limit; and input that is malformed, truncated, or in an unexpected encoding.
- Write the tests with the code, in the same change. A bug fix comes with a regression test (one
  that keeps the bug from coming back), which fails without the fix and passes with it; run it
  both ways.
- Add to the **Coverage exclusions** only code that can't run under test, such as generated code,
  and only after the user agrees. Never exclude code to reach the **Coverage floor**.
- When coverage rises above the **Coverage floor**, propose raising the floor to match, so it
  can't slip back.

## Write strict tests

- Assert the exact result: the whole value, the exact error type or variant, and the exact message
  when users see it. A test that checks only that nothing failed passes for most bugs.
- Test one behavior per test, and name each test for that behavior and its expected result, such
  as `rejects_empty_input`.
- Make tests deterministic: fix random seeds, pass the clock in, and use no network. Each test
  passes alone, in any order, and alongside the others.
- Test through the public interface. Replace only what the project doesn't control, such as the
  network or the clock. Prefer a fake (a small working version, such as an in-memory store) to a
  mock that only replays the calls it expects.
- Where a property holds for every input, such as `parse(format(x)) == x`, add a property-based
  test, which checks the property against many generated inputs.
- Also test the program the way people run it: the built binary with real arguments, the web API
  through real requests, or the app in a browser or on a device. Unit tests can't catch mistakes
  in how the parts are wired together.
- Don't ignore warnings the tests print. A deprecation or a leaked resource is often the next bug.

## Performance

- Enforce each of the **Performance budgets** with a test or a benchmark that fails the build
  when the code goes over it, not one that only prints a number.
- Measure on the hardware the budget names. Set each limit with room for that machine's normal
  variation, so a pass is reliable and a real slowdown still fails, and say in the test what
  hardware the limit assumes.
- Catch a slower algorithm, not only slower code. Use inputs large enough to show the difference
  between, for example, linear and quadratic time, or count the operations instead of timing
  them.
- Never raise a budget to make a test pass. A change that needs a higher budget is the user's
  decision, made with the measurements before and after.

## Security

- Test every place where data comes in from outside the program, such as files, the network,
  users, and environment variables, with hostile input: empty, oversized, malformed, truncated,
  deeply nested, or crafted to escape, such as `../` in a path or a quote in a query.
- Test that each limit holds: a size, depth, or rate limit rejects the first value over it.
- Test the checks that refuse: a user without permission is turned away, an expired token is
  rejected, and a bad signature fails.
- Fuzz every parser of outside data, as the **Security checks** describe. A fuzzer feeds the code
  a large number of generated inputs and reports each one that makes it crash or hang. Keep every
  input that ever crashed it as a regression test.
- Give every security fix a test that fails without the fix.
- Use generated values in fixtures, never real secrets, keys, or personal data.

## In continuous integration

- Run every test and check in the **CI** on every change, and make each one required before a
  merge. A failing check blocks the merge; never mark one as allowed to fail.
- Use the same commands and flags in CI as locally, so a pass in one means a pass in the other.
- Run the tests on every platform and version the project supports.

## Before finishing

1. Run the **Test commands**. Every test passes, and none is skipped that wasn't skipped before.
2. Run the **Coverage command**. Check that coverage meets the **Coverage floor**, and that every
   line and branch you added or changed is covered.
3. If the **Mutation command** isn't "none", run it, and give each change it reports as not
   caught a test or a reason.
4. Check the diff for weakened tests. The first command prints each marker you added that skips a
   test or runs one test alone, and the second prints each assertion you removed or changed. Both
   compare the working tree with the last commit, and both print nothing when there is nothing to
   report:

   ```bash
   git diff -U0 HEAD | grep -E \
     -e '^\+.*(#\[ignore|@Disabled|@Ignore|XCTSkip|t\.Skip\()' \
     -e '^\+.*@(pytest\.mark\.(skip|xfail)|unittest\.skip)' \
     -e '^\+.*(it|test|describe)\.(skip|only)\('
   ```

   ```bash
   git diff -U0 HEAD | grep -E '^-.*(assert|expect)'
   ```

   Add the markers of any other test framework the project uses. `git diff` leaves out files that
   Git doesn't track yet, so read new test files yourself. Read every line the commands print. A
   new skip, or an assertion removed or loosened, needs the user's agreement and a reason in your
   report; an assertion made stricter needs neither.
5. In your report, list the tests you added, each test you changed with the reason and who agreed,
   coverage before and after, and any check that couldn't run, with why.
