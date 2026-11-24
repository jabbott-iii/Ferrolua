# Security policy

Report a security vulnerability in Ferrolua privately, through GitHub's private vulnerability
reporting, never in public. This policy says how, what to include, and what happens next.

Ferrolua is designed to run Lua scripts that its hosts may not trust. A bug that lets a script
escape the sandbox, read or write memory it shouldn't reach, or run past the limits its host set
is a security vulnerability, and so is one that makes Ferrolua crash or hang on hostile input.

## Supported versions

Ferrolua has no release yet, so security fixes go to the `main` branch. Once there is a release,
the latest release line gets security fixes.

| Version | Security fixes |
| --- | --- |
| `main` | Yes |

## Report a vulnerability

1. Open the repository's **Security and quality** tab.
2. Select **Report a vulnerability**. It opens the
   [private report form](https://github.com/jabbott-iii/Ferrolua/security/advisories/new).

Never put a vulnerability in a public issue, discussion, or pull request, where anyone can read
it before a fix exists.

## What to include

- the commit or version you tested, and the platform it ran on
- the Lua script, host program, or steps that show the problem
- what an attacker could do with it, such as escape the sandbox, read or write memory, run code of
  their choice, or use more memory or time than the host allowed
- how to reach you with questions

Test only on your own systems and data, never against someone else's program that embeds
Ferrolua.

## What happens next

The project credits each reporter when the fix is published, unless the reporter asks not to be
named.

## Scope

This policy covers the code in this repository and its releases. It doesn't cover:

- a program that embeds Ferrolua: report a problem in that program to its own project
- another Lua implementation, such as PUC-Rio Lua or LuaJIT: report it to that project
- Rust's toolchain or standard library: follow the
  [Rust security policy](https://www.rust-lang.org/policies/security)
