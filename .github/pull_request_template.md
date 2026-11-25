## What changed and why

<!-- The change, and the problem it solves. Link the issue it closes: "Closes #12". Name the roadmap task, such as M1.3, if there is one. -->

## How it was checked

<!-- Each check you ran, and its result. -->

- [ ] `cargo fmt`, `cargo clippy`, `cargo test`, and `cargo doc` pass, as `CONTRIBUTING.md` shows
- [ ] `cargo xtask coverage` passes: 100% of lines and regions
- [ ] New behavior has tests, and a change to how a script runs has a conformance script with its
  Lua 5.5 twin, or an extension script
- [ ] Each new `unsafe` block has a `// SAFETY:` comment and an accepted decision record
- [ ] No third-party code was added

## Left for later

<!-- Breaking changes, with the decision record behind each; follow-up work; and anything a reviewer could miss. -->
