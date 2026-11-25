//! The property-test runner: it checks a property against generated cases, shrinks the first case
//! that fails, and reports the seed that generates that case again. The crate docs explain how it
//! shrinks a case.

use std::any::Any;
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::vec;

use crate::rng::Xoshiro256StarStar;

/// The settings of a property-test run: how many cases it checks, the seed of its first case, and
/// how many edits the runner may try while it shrinks a failing case.
///
/// Case `i` of a run uses the seed `seed + i`, wrapping past `u64::MAX` to 0, so the seed that a
/// [`Failure`] reports generates that case again as the first case of another run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    cases: u32,
    seed: u64,
    shrink_limit: u32,
}

impl Default for Config {
    /// [`Config::DEFAULT_CASES`] cases from [`Config::DEFAULT_SEED`], with at most
    /// [`Config::DEFAULT_SHRINK_LIMIT`] edits tried while shrinking.
    fn default() -> Self {
        Self {
            cases: Self::DEFAULT_CASES,
            seed: Self::DEFAULT_SEED,
            shrink_limit: Self::DEFAULT_SHRINK_LIMIT,
        }
    }
}

impl Config {
    /// The number of cases a run checks unless [`Config::with_cases`] sets another.
    pub const DEFAULT_CASES: u32 = 256;

    /// The seed of a run's first case unless [`Config::with_seed`] sets another.
    pub const DEFAULT_SEED: u64 = 0;

    /// The number of edits the runner may try while shrinking, unless
    /// [`Config::with_shrink_limit`] sets another.
    pub const DEFAULT_SHRINK_LIMIT: u32 = 10_000;

    /// Returns these settings with `cases` cases. A run with 0 cases checks nothing and passes.
    #[must_use]
    pub const fn with_cases(self, cases: u32) -> Self {
        Self { cases, ..self }
    }

    /// Returns these settings with `seed` as the seed of the first case.
    #[must_use]
    pub const fn with_seed(self, seed: u64) -> Self {
        Self { seed, ..self }
    }

    /// Returns these settings with at most `shrink_limit` edits tried while shrinking. With 0, a
    /// failing case is reported as it was generated.
    #[must_use]
    pub const fn with_shrink_limit(self, shrink_limit: u32) -> Self {
        Self {
            shrink_limit,
            ..self
        }
    }

    /// Checks `property` against the cases that `generate` builds, and shrinks the first case that
    /// fails.
    ///
    /// A case fails when `property` returns `false` or an `Err`, or panics. The runner catches the
    /// panic with [`std::panic::catch_unwind`], which needs the test to be built with
    /// `panic = "unwind"`, Cargo's default; the panic hook still prints each panic's message, so a
    /// failing test's output shows one for each panicking case the shrinker tried. Shrinking
    /// needs `generate` and `property` to be deterministic: given the same choices, `generate`
    /// builds the same case, and `property` gives the same outcome for it.
    ///
    /// # Errors
    ///
    /// Returns a [`Failure`] for the first case that fails, holding that case, the smallest failing
    /// case that shrinking found, and the seed.
    ///
    /// # Panics
    ///
    /// Panics when `generate` panics, because a generator that panics is a bug in the test.
    pub fn check<T, G, P, O>(&self, generate: G, property: P) -> Result<(), Failure<T>>
    where
        T: Clone,
        G: Fn(&mut Source) -> T,
        P: Fn(&T) -> O,
        O: Outcome,
    {
        for case in 0..self.cases {
            let seed = self.seed.wrapping_add(u64::from(case));
            let mut source = Source::random(seed);
            let original = generate(&mut source);
            if let Err(message) = evaluate(&property, &original) {
                let shrinker = Shrinker {
                    generate: &generate,
                    property: &property,
                    tries_left: self.shrink_limit,
                    choices: source.into_choices(),
                    case: original.clone(),
                    message,
                    shrinks: 0,
                }
                .run();
                return Err(Failure {
                    seed,
                    case,
                    original,
                    shrunk: shrinker.case,
                    message: shrinker.message,
                    shrinks: shrinker.shrinks,
                });
            }
        }
        Ok(())
    }

    /// Checks `property` as [`Config::check`] does, and panics with the report if a case fails.
    ///
    /// # Panics
    ///
    /// Panics when a case fails, with the [`Failure`]'s report as the message, and when `generate`
    /// panics.
    pub fn assert<T, G, P, O>(&self, generate: G, property: P)
    where
        T: Clone + fmt::Debug,
        G: Fn(&mut Source) -> T,
        P: Fn(&T) -> O,
        O: Outcome,
    {
        if let Err(failure) = self.check(generate, property) {
            panic!("{failure}");
        }
    }
}

/// The source of a generator's random choices, which records each choice so the runner can
/// replay and shrink the case.
///
/// Only the runner makes a source: [`Config::check`] passes one to the generator for each case.
#[derive(Debug)]
pub struct Source {
    choices: Choices,
    record: Vec<u64>,
}

/// Where a [`Source`] takes its choices from.
#[derive(Debug)]
enum Choices {
    /// A new case: each choice is drawn at random.
    Random(Xoshiro256StarStar),
    /// A case being shrunk: each choice is the next one of an edited list.
    Replay(vec::IntoIter<u64>),
}

impl Source {
    fn random(seed: u64) -> Self {
        Self {
            choices: Choices::Random(Xoshiro256StarStar::from_seed(seed)),
            record: Vec::new(),
        }
    }

    fn replay(choices: Vec<u64>) -> Self {
        Self {
            choices: Choices::Replay(choices.into_iter()),
            record: Vec::new(),
        }
    }

    fn into_choices(self) -> Vec<u64> {
        self.record
    }

    /// Chooses a number from 0 up to but not including `bound`.
    ///
    /// In a new case, every number in the range is equally likely. While shrinking, the choice
    /// comes from the edited list instead: an edit can leave a number at or above `bound`, which
    /// is reduced modulo `bound`, and a call past the end of the list chooses 0.
    ///
    /// To build a collection that shrinks well, choose whether to add another element before each
    /// one, with 0 meaning stop, rather than choosing its length first. Deleting the choices of
    /// one element then deletes that element.
    ///
    /// # Panics
    ///
    /// Panics if `bound` is 0, because no number is below 0.
    pub fn below(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "`Source::below` needs a bound above 0");
        let choice = match &mut self.choices {
            Choices::Random(generator) => generator.below(bound),
            Choices::Replay(rest) => rest.next().map_or(0, |choice| choice % bound),
        };
        self.record.push(choice);
        choice
    }
}

/// What a property returns: `true` or `Ok(())` when the case passes, and `false` or an `Err` when
/// it fails.
pub trait Outcome {
    /// Converts the outcome into `Ok(())` for a pass, or into an `Err` holding a message that says
    /// why the case fails.
    ///
    /// # Errors
    ///
    /// Returns the message when the case fails.
    fn into_result(self) -> Result<(), String>;
}

impl Outcome for bool {
    fn into_result(self) -> Result<(), String> {
        if self {
            Ok(())
        } else {
            Err(String::from("the property returned false"))
        }
    }
}

impl<E: fmt::Display> Outcome for Result<(), E> {
    fn into_result(self) -> Result<(), String> {
        self.map_err(|error| format!("the property returned an error: {error}"))
    }
}

/// The report of a failed property: the case that failed first, the smallest failing case that
/// shrinking found from it, and the seed that generates the first one again.
///
/// Its `Display` text is the report that [`Config::assert`] panics with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure<T> {
    /// The seed of the failing case. A run with this seed generates the case again as its first
    /// case.
    pub seed: u64,
    /// The failing case's position in its run, counting from 0.
    pub case: u32,
    /// The case as it was generated.
    pub original: T,
    /// The smallest failing case that shrinking found, which is `original` when shrinking found
    /// none smaller.
    pub shrunk: T,
    /// Why `shrunk` fails: the property returned `false`, returned an error, or panicked.
    pub message: String,
    /// How many times shrinking found a smaller failing case.
    pub shrinks: u32,
}

impl<T: fmt::Debug> fmt::Display for Failure<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            seed,
            case,
            original,
            shrunk,
            message,
            shrinks,
        } = self;
        write!(
            f,
            "property failed on case {case} (seed {seed})\n  \
             reason: {message}\n  \
             shrunk case: {shrunk:?}\n  \
             original case: {original:?}\n  \
             smaller failing cases found: {shrinks}\n  \
             to rerun this case alone, add `.with_seed({seed}).with_cases(1)` to the run's `Config`"
        )
    }
}

impl<T: fmt::Debug> std::error::Error for Failure<T> {}

/// Runs `property` on `case`, turning a panic into a failure with the panic's message.
fn evaluate<T, P, O>(property: &P, case: &T) -> Result<(), String>
where
    P: Fn(&T) -> O,
    O: Outcome,
{
    // `AssertUnwindSafe` is sound here: after a panic, the runner reads nothing the property
    // touched, and a property that keeps state across cases already breaks shrinking.
    match panic::catch_unwind(AssertUnwindSafe(|| property(case))) {
        Ok(outcome) => outcome.into_result(),
        Err(payload) => Err(panic_message(payload.as_ref())),
    }
}

/// Describes a panic from its payload: the message of `panic!`, which is a `&str` or a `String`.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        format!("the property panicked: {message}")
    } else if let Some(message) = payload.downcast_ref::<String>() {
        format!("the property panicked: {message}")
    } else {
        String::from("the property panicked with a payload that isn't a string")
    }
}

/// The state of shrinking one failing case: the smallest failing case so far, its choices, and the
/// edits left to try.
struct Shrinker<'a, T, G, P> {
    generate: &'a G,
    property: &'a P,
    tries_left: u32,
    choices: Vec<u64>,
    case: T,
    message: String,
    shrinks: u32,
}

impl<T, G, P, O> Shrinker<'_, T, G, P>
where
    G: Fn(&mut Source) -> T,
    P: Fn(&T) -> O,
    O: Outcome,
{
    /// Repeats both kinds of edit until neither finds a smaller failing case, or no tries are left.
    fn run(mut self) -> Self {
        loop {
            let shrinks_before = self.shrinks;
            self.delete_runs();
            self.lower_each();
            if self.shrinks == shrinks_before || self.tries_left == 0 {
                return self;
            }
        }
    }

    /// Tries deleting each run of 8, 4, 2, and 1 consecutive choices.
    fn delete_runs(&mut self) {
        for length in [8, 4, 2, 1] {
            let mut start = 0;
            while start + length <= self.choices.len() {
                let mut candidate = self.choices.clone();
                candidate.drain(start..start + length);
                if self.try_choices(candidate.clone()) {
                    continue;
                }
                // The choice before a run often counts the elements the run builds, so deleting
                // the run may only work if that count goes down too.
                if let Some(count) = start
                    .checked_sub(1)
                    .and_then(|before| candidate.get_mut(before))
                    && *count > 0
                {
                    *count -= 1;
                    if self.try_choices(candidate) {
                        continue;
                    }
                }
                start += 1;
            }
        }
    }

    /// Lowers each choice in turn as far as it goes while the case still fails.
    fn lower_each(&mut self) {
        let mut index = 0;
        while index < self.choices.len() {
            self.lower(index);
            index += 1;
        }
    }

    /// Searches for the smallest value of choice `index` that still fails, halving the range of
    /// values left to try with each edit. It assumes that the case passes for every value below
    /// one that passes, which holds for a threshold such as `x > 1000`.
    fn lower(&mut self, index: usize) {
        let mut low = 0;
        while let Some(&high) = self.choices.get(index) {
            if low >= high {
                return;
            }
            let middle = low + (high - low) / 2;
            let mut candidate = self.choices.clone();
            candidate[index] = middle;
            if !self.try_choices(candidate) {
                low = middle + 1;
            }
        }
    }

    /// Runs the generator on `candidate` and keeps the case it builds if that case fails and the
    /// choices it used are smaller than the current ones. Returns whether it kept the case, which
    /// it doesn't once no tries are left.
    fn try_choices(&mut self, candidate: Vec<u64>) -> bool {
        if self.tries_left > 0 {
            self.tries_left -= 1;
            let mut source = Source::replay(candidate);
            let case = (self.generate)(&mut source);
            let choices = source.into_choices();
            if (choices.len(), &choices) < (self.choices.len(), &self.choices)
                && let Err(message) = evaluate(self.property, &case)
            {
                self.choices = choices;
                self.case = case;
                self.message = message;
                self.shrinks += 1;
                return true;
            }
        }
        false
    }
}
