//! `lunaferra-jit` will be Lunaferra's just-in-time (JIT) compiler, which turns hot functions into
//! x86-64 and AArch64 machine code: its encoders, its executable memory, and its baseline and
//! optimizing tiers. Hosts will reach it through the `jit` feature of the `lunaferra` crate, which
//! is off by default, as the [JIT spec] describes.
//!
//! The crate holds no code yet. Milestone M5 of the [roadmap], which runs hot code as x86-64
//! machine code, starts it, and milestone M13 adds AArch64.
//!
//! [JIT spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/jit.md
//! [roadmap]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/roadmap.md
