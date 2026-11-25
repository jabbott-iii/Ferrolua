//! `ferrolua-syntax` will hold Ferrolua's lexer and parser, its syntax trees and source spans, and
//! the diagnostic type that every tool shares. The compiler and the analysis tools will all read
//! the trees it builds, as the [syntax spec] describes.
//!
//! The crate holds no code yet. Milestone M1 of the [roadmap], which parses Ferrolua, starts it.
//!
//! [syntax spec]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/specs/syntax.md
//! [roadmap]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/roadmap.md
