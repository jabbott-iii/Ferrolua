# Subsystem: `ferrolua-dap`

`ferrolua-dap` is Ferrolua's debug adapter. It speaks the Debug Adapter Protocol (DAP) so editors
can set breakpoints in Lua scripts, step through them, and inspect frames and variables (ADR-0015,
proposed). `ferrolua dap` launches a script under the debugger over standard input and output. A
host can also serve the protocol over a local TCP connection, so a developer can attach to scripts
running inside a game or a server. It uses the VM's debug hooks ([`vm.md`](vm.md#debug-hooks)) and
the `ferrolua` crate's debugger interface. It lands in M10.

## Why debugging inside the host matters

Embedded scripts rarely run alone: they depend on the functions and data their host provides. A
debugger that can only launch a script from the command line can't reproduce that. Letting the
host serve the protocol means the script is debugged where it runs, with the host's
globals and state. The host decides whether to open the port; it is off unless the host turns it
on, and listens only on the loopback address.

## Features

| Feature | DAP requests |
| :- | :- |
| Launch a script, or attach to a host | `launch`, `attach`, `disconnect` |
| Line breakpoints, with conditions, hit counts, and log messages | `setBreakpoints` |
| Stop on errors | `setExceptionBreakpoints` |
| Pause, continue, step in, over, and out | `pause`, `continue`, `stepIn`, `next`, `stepOut` |
| Call stack, scopes, and variables, with tables expanded on demand | `stackTrace`, `scopes`, `variables` |
| Evaluate an expression in a frame | `evaluate` |
| Change a variable's value | `setVariable` |

- A function with a breakpoint runs interpreted, so the JIT never needs to stop inside machine
  code.
- `evaluate` compiles the expression with the frame's locals and upvalues in scope and runs it under
  the state's limits. An expression can call functions, so the editor's documentation for hover
  evaluation warns that it may have side effects.
- While a script is stopped, the host's thread waits inside the debug hook, reading messages, so
  the adapter needs no thread of its own.

## Invariants

1. Every request gets a response, and no message makes the adapter panic.
2. A breakpoint on a line stops before that line's first instruction, every time it runs.
3. Stepping over a call never stops inside it, unless a breakpoint there is hit.
4. Variables shown for a frame are exactly the locals and upvalues in scope at the stopped
   instruction.

## Tests

- Invariant 1: the `dap` fuzz target and hostile-input tests.
- Invariants 2–4: end-to-end tests launch `ferrolua dap` as a process, drive it with recorded
  message sequences, and compare each response with an expected file.
