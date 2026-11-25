//! `ferrolua-testkit` will hold the test tools that Ferrolua writes for itself: a seeded random
//! generator, a property-test runner that shrinks a failing case, and a generator of Ferrolua
//! programs. The other crates will use it only as a dev-dependency, as the [quality document]
//! describes.
//!
//! The crate holds no code yet. Task M0.2 of the [roadmap], which builds the generator and the
//! property-test runner, starts it.
//!
//! [quality document]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/04-quality.md#ferrolua-testkit
//! [roadmap]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/roadmap.md
