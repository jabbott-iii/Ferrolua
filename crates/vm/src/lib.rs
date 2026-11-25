//! `lunaferra-vm` will run Lunaferra's bytecode. It will hold the heap, with its reference counts
//! and its loop collector, and the values, tables, interpreter, metamethods, coroutines, limits,
//! and debug hooks, as the [VM spec] and the [memory spec] describe.
//!
//! The crate holds no code yet. Task M2.2 of the [roadmap], which builds the heap, starts it.
//!
//! [VM spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/vm.md
//! [memory spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/memory.md
//! [roadmap]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/roadmap.md
