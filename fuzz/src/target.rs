//! The fuzz targets: the functions the fuzzer feeds inputs to, each under the name that
//! `cargo xtask fuzz <target>` takes.
//!
//! A target fails an input by panicking, by crashing the process, or by running past the time
//! limit. It must keep no state from one input to the next, because a child process runs many
//! inputs one after another, and the fuzzer runs a failing input again alone to shrink it.
//!
//! The two fixtures fail on purpose, so the fuzzer's own tests can check that it finds, shrinks,
//! saves, and replays a failure. The targets that check Ferrolua's own code arrive with the code
//! they check, starting with the lexer and the parser in task M1.9.

use std::fmt;

/// A function the fuzzer feeds inputs to.
#[derive(Debug)]
pub(crate) struct Target {
    /// The name that `cargo xtask fuzz` takes, which also names the target's folders in
    /// `fuzz/corpus/` and `fuzz/regressions/`.
    pub(crate) name: &'static str,
    /// What the target does, for the usage text.
    pub(crate) about: &'static str,
    /// How every saved input of the target must run when it is replayed. For a target that checks
    /// Ferrolua's code, that is [`Kind::Passed`], because each saved input is a bug since fixed. A
    /// fixture fails on purpose, so its saved inputs must fail as it is built to.
    pub(crate) replays_as: Kind,
    /// Runs the target on one input.
    pub(crate) run: fn(&[u8]),
}

/// Targets are equal when their names are, because a name picks out one target.
impl PartialEq for Target {
    fn eq(&self, other: &Target) -> bool {
        self.name == other.name
    }
}

impl Eq for Target {}

/// How a run of a target on one input ended, without its details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The target returned.
    Passed,
    /// The target panicked.
    Panicked,
    /// The target ran past the time limit.
    Hung,
    /// The process stopped with a status that isn't a caught panic, such as an abort.
    Crashed,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Passed => "passed",
            Kind::Panicked => "panicked",
            Kind::Hung => "hung",
            Kind::Crashed => "crashed",
        })
    }
}

/// Every target, in the order the usage text lists them.
pub(crate) const TARGETS: &[Target] = &[
    Target {
        name: "fixture-panic",
        about: "panics on an input that starts with FUZZ, to test the fuzzer",
        replays_as: Kind::Panicked,
        run: fixture_panic,
    },
    Target {
        name: "fixture-hang",
        about: "loops forever when its input's bytes link back, to test the fuzzer",
        replays_as: Kind::Hung,
        run: fixture_hang,
    },
];

/// Returns the target named `name`.
pub(crate) fn find(name: &str) -> Option<&'static Target> {
    TARGETS.iter().find(|target| target.name == name)
}

/// Panics on an input that starts with `FUZZ`, and passes every other input.
fn fixture_panic(input: &[u8]) {
    assert!(!input.starts_with(b"FUZZ"), "the input starts with FUZZ");
}

/// Follows a chain of links through the input, where each byte is the index of the next byte,
/// starting from byte 0 and stopping at an index past the end.
///
/// Nothing bounds the number of steps, so a chain that comes back to a byte it has visited, such
/// as the single byte 0, loops forever. That is the bug a fuzzer should find: `.claude/rules/
/// rust.md` asks every loop that follows links inside its input to bound its steps.
fn fixture_hang(input: &[u8]) {
    let mut index = 0;
    while let Some(&next) = input.get(index) {
        index = usize::from(next);
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, TARGETS, find};

    #[test]
    fn finds_each_target_by_its_name() {
        for target in TARGETS {
            assert_eq!(find(target.name).map(|found| found.name), Some(target.name));
        }
    }

    #[test]
    fn finds_no_target_for_an_unknown_name() {
        assert!(find("fixture").is_none());
    }

    #[test]
    fn fixture_panic_passes_an_input_without_the_prefix() {
        (find("fixture-panic").unwrap().run)(b"FUZ");
        (find("fixture-panic").unwrap().run)(b"xFUZZ");
    }

    #[test]
    #[should_panic(expected = "the input starts with FUZZ")]
    fn fixture_panic_panics_on_an_input_with_the_prefix() {
        (find("fixture-panic").unwrap().run)(b"FUZZ!");
    }

    #[test]
    fn fixture_hang_stops_when_a_link_points_past_the_end() {
        // 0 → 2 → 1 → 3, which is past the end.
        (find("fixture-hang").unwrap().run)(&[2, 3, 1]);
        (find("fixture-hang").unwrap().run)(&[]);
    }

    #[test]
    fn names_each_kind_of_ending() {
        let names =
            [Kind::Passed, Kind::Panicked, Kind::Hung, Kind::Crashed].map(|kind| kind.to_string());
        assert_eq!(names, ["passed", "panicked", "hung", "crashed"]);
    }
}
