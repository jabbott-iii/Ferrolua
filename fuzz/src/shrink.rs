//! Shrinking: from an input that fails, find a smaller one that fails the same way, so the saved
//! input shows the bug with as little else as possible.
//!
//! An input is smaller than another when it is shorter, or as long but with a smaller byte where
//! the two first differ. The shrinker tries two kinds of edit, and keeps each one after which the
//! input still fails in the same kind of way, as [`Kind`](crate::target::Kind) names it:
//!
//! 1. It deletes runs of bytes, from the longest run that fits down to single bytes.
//! 2. It lowers each byte toward 0, halving the range of values left to try with each edit.
//!
//! It repeats both until neither finds a smaller failing input, or until it has tried as many
//! edits as its limit allows. The result is never larger than the input it started from, but it
//! isn't always the smallest input that fails.

use crate::runner::Failure;

/// The smallest failing input that shrinking found, and how it fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shrunk {
    /// The input.
    pub(crate) input: Vec<u8>,
    /// How it fails.
    pub(crate) failure: Failure,
}

/// Shrinks `input`, which fails as `failure` says, trying at most `limit` edits.
///
/// `fails` runs a candidate and returns how it fails, or `None` when it passes.
///
/// # Errors
///
/// Returns the first error that `fails` returns.
pub(crate) fn shrink<E>(
    input: Vec<u8>,
    failure: Failure,
    limit: u32,
    fails: impl FnMut(&[u8]) -> Result<Option<Failure>, E>,
) -> Result<Shrunk, E> {
    let mut shrinker = Shrinker {
        input,
        failure,
        tries_left: limit,
        fails,
    };
    loop {
        let input_before = shrinker.input.clone();
        shrinker.delete_runs()?;
        shrinker.lower_each()?;
        if shrinker.input == input_before || shrinker.tries_left == 0 {
            return Ok(Shrunk {
                input: shrinker.input,
                failure: shrinker.failure,
            });
        }
    }
}

/// The state of shrinking one input: the smallest failing input so far, how it fails, and the
/// edits left to try.
struct Shrinker<F> {
    input: Vec<u8>,
    failure: Failure,
    tries_left: u32,
    fails: F,
}

impl<E, F: FnMut(&[u8]) -> Result<Option<Failure>, E>> Shrinker<F> {
    /// Tries deleting each run of bytes whose length is a power of 2, from the longest that fits
    /// down to 1. After a deletion that it keeps, it tries the bytes that moved into the run's
    /// place.
    fn delete_runs(&mut self) -> Result<(), E> {
        let mut length = self.input.len().checked_ilog2().map_or(0, |log| 1 << log);
        while length > 0 {
            let mut start = 0;
            while start + length <= self.input.len() {
                let mut candidate = self.input.clone();
                candidate.drain(start..start + length);
                if !self.keep(candidate)? {
                    start += length;
                }
            }
            length /= 2;
        }
        Ok(())
    }

    /// Lowers each byte in turn as far as it goes while the input still fails.
    fn lower_each(&mut self) -> Result<(), E> {
        for index in 0..self.input.len() {
            self.lower(index)?;
        }
        Ok(())
    }

    /// Searches for the smallest value of byte `index` that still fails, halving the range of
    /// values left to try with each edit. It assumes that the input passes for every value below
    /// one that passes, which holds for a comparison such as `byte > 100`.
    fn lower(&mut self, index: usize) -> Result<(), E> {
        let mut low = 0;
        while low < self.input[index] {
            let middle = low + (self.input[index] - low) / 2;
            let mut candidate = self.input.clone();
            candidate[index] = middle;
            if !self.keep(candidate)? {
                low = middle + 1;
            }
        }
        Ok(())
    }

    /// Runs `candidate`, which is smaller than the current input, and keeps it when it fails the
    /// same way. Returns whether it kept it, which it doesn't once no tries are left.
    fn keep(&mut self, candidate: Vec<u8>) -> Result<bool, E> {
        if self.tries_left == 0 {
            return Ok(false);
        }
        self.tries_left -= 1;
        match (self.fails)(&candidate)? {
            Some(failure) if failure.kind() == self.failure.kind() => {
                self.input = candidate;
                self.failure = failure;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::Duration;

    use super::{Shrunk, shrink};
    use crate::runner::Failure;

    const LIMIT: Duration = Duration::from_millis(250);

    fn panic() -> Failure {
        Failure::Panic("the input starts with FUZZ".to_owned())
    }

    /// Fails as `fixture-panic` does: on an input that starts with `FUZZ`.
    fn starts_with_fuzz(candidate: &[u8]) -> Result<Option<Failure>, ()> {
        Ok(candidate.starts_with(b"FUZZ").then(panic))
    }

    #[test]
    fn deletes_everything_after_the_bytes_that_fail() {
        let mut input = b"FUZZ".to_vec();
        input.extend(0..200_u8);
        let shrunk = shrink(input, panic(), 10_000, starts_with_fuzz).unwrap();
        assert_eq!(
            shrunk,
            Shrunk {
                input: b"FUZZ".to_vec(),
                failure: panic()
            }
        );
    }

    #[test]
    fn deletes_bytes_before_and_between_the_bytes_that_fail() {
        // Fails when the input holds a 7 somewhere after a 9.
        let fails = |candidate: &[u8]| -> Result<Option<Failure>, ()> {
            let nine = candidate.iter().position(|&byte| byte == 9);
            Ok(nine
                .is_some_and(|nine| candidate[nine..].contains(&7))
                .then(panic))
        };
        let shrunk = shrink(vec![1, 9, 2, 3, 7, 4, 5], panic(), 10_000, fails).unwrap();
        assert_eq!(shrunk.input, [9, 7]);
    }

    #[test]
    fn lowers_each_byte_to_the_smallest_value_that_fails() {
        // Fails when the first byte is above 100, so it shrinks to 101.
        let fails = |candidate: &[u8]| -> Result<Option<Failure>, ()> {
            Ok(candidate
                .first()
                .is_some_and(|&byte| byte > 100)
                .then(panic))
        };
        let shrunk = shrink(vec![250, 250], panic(), 10_000, fails).unwrap();
        assert_eq!(shrunk.input, [101]);
    }

    #[test]
    fn keeps_the_failure_of_the_smallest_input() {
        let fails = |candidate: &[u8]| -> Result<Option<Failure>, ()> {
            Ok((!candidate.is_empty())
                .then(|| Failure::Panic(format!("length {}", candidate.len()))))
        };
        let shrunk = shrink(
            vec![5; 6],
            Failure::Panic("length 6".to_owned()),
            10_000,
            fails,
        )
        .unwrap();
        assert_eq!(
            shrunk,
            Shrunk {
                input: vec![0],
                failure: Failure::Panic("length 1".to_owned())
            }
        );
    }

    #[test]
    fn rejects_a_smaller_input_that_fails_another_way() {
        // Every smaller input hangs instead of panicking, so none is kept.
        let fails = |_: &[u8]| -> Result<Option<Failure>, ()> { Ok(Some(Failure::Hang(LIMIT))) };
        let shrunk = shrink(vec![0, 0, 0], panic(), 10_000, fails).unwrap();
        assert_eq!(shrunk.input, [0, 0, 0]);
    }

    #[test]
    fn stops_after_its_limit_of_tries() {
        let tries = Cell::new(0);
        let fails = |candidate: &[u8]| -> Result<Option<Failure>, ()> {
            tries.set(tries.get() + 1);
            starts_with_fuzz(candidate)
        };
        let mut input = b"FUZZ".to_vec();
        input.extend([1; 60]);
        let shrunk = shrink(input.clone(), panic(), 3, fails).unwrap();
        assert_eq!(tries.get(), 3);
        // Deleting all 64 bytes passes, and so does deleting the first 32. Deleting the last 32
        // fails, and that is the last try.
        assert_eq!(shrunk.input, input[..32]);
    }

    #[test]
    fn keeps_an_input_with_nothing_to_delete_or_lower() {
        // The empty input has no edit to try, so `starts_with_fuzz` never runs.
        let shrunk = shrink(Vec::new(), panic(), 10_000, starts_with_fuzz).unwrap();
        assert_eq!(
            shrunk,
            Shrunk {
                input: Vec::new(),
                failure: panic()
            }
        );
    }

    #[test]
    fn stops_at_an_error_while_deleting() {
        let error = shrink(vec![1, 2], panic(), 10_000, |_| Err("no child")).unwrap_err();
        assert_eq!(error, "no child");
    }

    #[test]
    fn stops_at_an_error_while_lowering() {
        // Deleting the only byte passes, so the next edit lowers it, and that fails to run.
        let fails = |candidate: &[u8]| {
            if candidate.is_empty() {
                Ok(None)
            } else {
                Err("no child")
            }
        };
        assert_eq!(
            shrink(vec![9], panic(), 10_000, fails).unwrap_err(),
            "no child"
        );
    }
}
