//! `lunaferra-analysis` will build the semantic model that the type checker, the linter, the
//! security scanner, and the language server share: names and scopes, control flow, data flow, and
//! the files a project links with `require`, as the [analysis spec] describes.
//!
//! The crate holds no code yet. Task M6.1 of the [roadmap], which resolves names, scopes, and
//! symbols, starts it.
//!
//! [analysis spec]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/specs/analysis.md
//! [roadmap]: https://github.com/jabbott-iii/Lunaferra/blob/main/docs/roadmap.md
