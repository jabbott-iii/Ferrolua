//! `lunaferra-sast` will be Lunaferra's static application security testing (SAST) tool. It will
//! follow untrusted data across functions and files to the dangerous calls it reaches, and report
//! what it finds in SARIF, the Static Analysis Results Interchange Format, as the [SAST spec]
//! describes.
//!
//! The crate holds no code yet. Task M8.1 of the [roadmap], which defines taint sources, sinks, and
//! sanitizers, starts it.
//!
//! [SAST spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/sast.md
//! [roadmap]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/roadmap.md
