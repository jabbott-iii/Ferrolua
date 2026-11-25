//! `ferrolua-compiler` will turn the compact syntax tree of `ferrolua-syntax` into the bytecode of
//! `ferrolua-bytecode`. It will resolve names and scopes, allocate registers, and apply strict
//! mode, as the [compiler spec] describes.
//!
//! The crate holds no code yet. Task M2.5 of the [roadmap], which resolves names, upvalues, and
//! globals, starts it.
//!
//! [compiler spec]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/specs/compiler.md
//! [roadmap]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/roadmap.md
