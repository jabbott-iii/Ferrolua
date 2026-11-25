//! `ferrolua-testkit` holds the test tools that Ferrolua writes for itself: a seeded random
//! generator, and a property-test runner that shrinks a failing case. ADR-0002 (Ferrolua uses no
//! third-party code) rules out test libraries, so these take their place.
//!
//! The other crates use it only as a dev-dependency, and it is never published, as the [quality
//! document] describes. It depends on nothing but the standard library. A generator of Ferrolua
//! programs joins it in task M1.9 of the [roadmap], which adds the fuzz targets for the lexer and
//! the parser.
//!
//! # The parts
//!
//! - Random numbers: [`SplitMix64`] expands a 64-bit seed, and [`Xoshiro256StarStar`] generates
//!   from the expanded state. Both give the outputs that their authors' reference code gives.
//! - Property tests: [`Config`] checks a property against generated cases. A generator builds each
//!   case from the choices of a [`Source`], a property returns an [`Outcome`], and a failed check
//!   returns a [`Failure`], or panics with its report.
//!
//! # Example
//!
//! ```
//! use ferrolua_testkit::Config;
//!
//! // Reversing a list twice gives back the list.
//! Config::default().assert(
//!     |source| {
//!         let mut list = Vec::new();
//!         // Each 0 ends the list, so shrinking toward 0 gives shorter lists.
//!         while source.below(4) != 0 {
//!             list.push(source.below(100));
//!         }
//!         list
//!     },
//!     |list| {
//!         let mut twice = list.clone();
//!         twice.reverse();
//!         twice.reverse();
//!         twice == *list
//!     },
//! );
//! ```
//!
//! # How cases are generated and shrunk
//!
//! A generator is a function that builds a case from the random choices of a [`Source`]. The
//! source records each choice, so the runner knows every case by its list of choices as well as by
//! its value. To shrink a failing case, the runner edits the list: it deletes runs of choices, and
//! it lowers each choice toward zero. Then it runs the generator again on the edited list. It keeps
//! an edit when the property still fails and the list the generator used is smaller: shorter, or
//! as long but with a smaller choice where the two lists first differ.
//!
//! Each generator shrinks this way without a shrinking function of its own, as long as choices
//! nearer zero give simpler cases: smaller numbers, shorter lists, and earlier alternatives.
//! Hypothesis, a property-testing library for Python, also shrinks the list of choices instead of
//! the case. A shrunk case is never larger than the original, but it isn't always the smallest
//! case that fails.
//!
//! # Determinism
//!
//! A run gives the same cases, the same shrunk case, and the same report every time, on every
//! target, as long as the generator and the property are deterministic. [`Config`] says which
//! seed each case uses, and [`Failure`] reports the seed of the case that failed.
//!
//! [quality document]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/04-quality.md#ferrolua-testkit
//! [roadmap]: https://github.com/jabbott-iii/Ferrolua/blob/main/docs/roadmap.md

mod property;
mod rng;

pub use property::{Config, Failure, Outcome, Source};
pub use rng::{SplitMix64, Xoshiro256StarStar};
