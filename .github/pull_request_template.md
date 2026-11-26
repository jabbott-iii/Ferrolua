## What changed and why

<!-- The change, and the problem it solves. Link the issue it closes: "Closes #12". Name the roadmap task, such as M1.3, if there is one. -->

## How it was checked

<!-- CI runs every check in `CONTRIBUTING.md` on this pull request, one job each, and fuzzes each fuzz target. List any other check you ran, and its result. -->

- [ ] The checks in `CONTRIBUTING.md` pass on your machine, including `cargo xtask coverage` at 100% of lines and regions
- [ ] New behavior has tests, and a change to how a script runs has a conformance script with its
  Lua 5.5 twin, or an extension script
- [ ] Each new `unsafe` block has a `// SAFETY:` comment and an accepted decision record
- [ ] No third-party code was added

## Left for later

<!-- Breaking changes, with the decision record behind each; follow-up work; and anything a reviewer could miss. -->
