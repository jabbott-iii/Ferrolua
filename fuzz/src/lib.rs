//! `ferrolua-fuzz` will be Ferrolua's fuzzer, which feeds generated and mutated inputs from a fixed
//! seed to its targets, such as the parser, the compiler, and the VM, and saves every input that
//! crashes one, as the [quality document] describes.
//!
//! The crate holds no code yet. Task M0.4 of the [roadmap], which builds the fuzzer and replays its
//! saved inputs, starts it.
//!
//! [quality document]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/04-quality.md#ferrolua-fuzz
//! [roadmap]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/roadmap.md
