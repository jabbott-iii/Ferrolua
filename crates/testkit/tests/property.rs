//! Tests of the property-test runner through its public API: which cases it generates, how it
//! shrinks a failing one, and what it reports.
//!
//! A case's first choice is known from the reference outputs in `random.rs`: for a bound of
//! `u64::MAX`, an output `x` above 0 gives `x - 1`. Seed 0's first output is
//! 11,091,344,671,253,066,420 and seed 41's is 13,225,511,433,842,405,998.
//!
//! A pair of tests that pass and fail with the same generator and property name them as functions
//! instead of writing closures. Each closure has a type of its own, so the pair then runs one
//! instantiation of the generic `Config::check` or `Config::assert` both ways, and `llvm-cov`
//! scores a generic function by its instantiation that covers the most regions.

use std::cell::Cell;
use std::panic;

use ferrolua_testkit::{Config, Failure, Outcome, Source};

/// Chooses a number from 0 up to but not including `u64::MAX`.
fn any_number(source: &mut Source) -> u64 {
    source.below(u64::MAX)
}

/// Chooses a digit. From seed 2, the first five cases choose 1, 6, 2, 2, and 7.
fn digit(source: &mut Source) -> u64 {
    source.below(10)
}

/// Holds for every digit but 7.
fn not_7(&digit: &u64) -> bool {
    digit != 7
}

/// Chooses a list of numbers below 1,000, choosing its length first.
fn list_with_length_first(source: &mut Source) -> Vec<u64> {
    let length = source.below(20);
    (0..length).map(|_| source.below(1000)).collect()
}

/// Chooses a list of numbers below 1,000, choosing whether to add each element before it.
fn list_with_a_choice_per_element(source: &mut Source) -> Vec<u64> {
    let mut list = Vec::new();
    while source.below(4) != 0 {
        list.push(source.below(1000));
    }
    list
}

/// Builds the failure that `Config::check` should return.
fn failure<T>(
    seed: u64,
    case: u32,
    original: T,
    shrunk: T,
    message: &str,
    shrinks: u32,
) -> Failure<T> {
    Failure {
        seed,
        case,
        original,
        shrunk,
        message: String::from(message),
        shrinks,
    }
}

// Halving the range of numbers left to try finds a smaller failing number 61 times on the way
// down from the original to 1001.
#[test]
fn shrinks_a_failure_above_1000_to_1001() {
    let result = Config::default().check(any_number, |&number| number <= 1000);
    assert_eq!(
        result,
        Err(failure(
            0,
            0,
            11_091_344_671_253_066_419,
            1001,
            "the property returned false",
            61
        ))
    );
}

#[test]
fn assert_panics_with_a_report_that_gives_the_seed() {
    let payload = panic::catch_unwind(|| {
        Config::default()
            .with_seed(41)
            .assert(any_number, |&number| number <= 1000);
    })
    .unwrap_err();
    assert_eq!(
        payload.downcast_ref::<String>().unwrap(),
        "property failed on case 0 (seed 41)\n\
         \x20 reason: the property returned false\n\
         \x20 shrunk case: 1001\n\
         \x20 original case: 13225511433842405997\n\
         \x20 smaller failing cases found: 58\n\
         \x20 to rerun this case alone, add `.with_seed(41).with_cases(1)` to the run's `Config`"
    );
}

#[test]
fn assert_panics_with_the_report_of_a_later_case() {
    let payload =
        panic::catch_unwind(|| Config::default().with_seed(2).assert(digit, not_7)).unwrap_err();
    assert_eq!(
        payload.downcast_ref::<String>().unwrap(),
        "property failed on case 4 (seed 6)\n\
         \x20 reason: the property returned false\n\
         \x20 shrunk case: 7\n\
         \x20 original case: 7\n\
         \x20 smaller failing cases found: 0\n\
         \x20 to rerun this case alone, add `.with_seed(6).with_cases(1)` to the run's `Config`"
    );
}

#[test]
fn assert_returns_when_every_case_passes() {
    Config::default()
        .with_seed(2)
        .with_cases(4)
        .assert(digit, not_7);
}

#[test]
fn a_passing_run_generates_the_default_256_cases() {
    let generated = Cell::new(0);
    let result = Config::default().check(
        |source| {
            generated.set(generated.get() + 1);
            source.below(10)
        },
        |&number| number < 10,
    );
    assert_eq!(result, Ok(()));
    assert_eq!(generated.get(), 256);
}

#[test]
fn a_run_of_0_cases_passes_without_generating_a_case() {
    let generated = Cell::new(0);
    let result = Config::default().with_cases(0).check(
        |source| {
            generated.set(generated.get() + 1);
            source.below(10)
        },
        |_| false,
    );
    assert_eq!(result, Ok(()));
    assert_eq!(generated.get(), 0);
}

#[test]
fn the_defaults_are_256_cases_from_seed_0_with_10000_tries() {
    assert_eq!(
        (
            Config::DEFAULT_CASES,
            Config::DEFAULT_SEED,
            Config::DEFAULT_SHRINK_LIMIT
        ),
        (256, 0, 10_000)
    );
    assert_eq!(
        Config::default(),
        Config::default()
            .with_cases(256)
            .with_seed(0)
            .with_shrink_limit(10_000)
    );
}

// Every edit of the 7 gives a digit that passes, so shrinking keeps it.
#[test]
fn reports_a_later_case_with_its_own_seed() {
    let result = Config::default().with_seed(2).check(digit, not_7);
    assert_eq!(
        result,
        Err(failure(6, 4, 7, 7, "the property returned false", 0))
    );
}

#[test]
fn a_run_checks_only_its_number_of_cases() {
    let result = Config::default()
        .with_seed(2)
        .with_cases(4)
        .check(digit, not_7);
    assert_eq!(result, Ok(()));
}

#[test]
fn the_reported_seed_generates_the_case_again_as_the_first_case() {
    let property = |&number: &u64| number <= 1000;
    let first = Config::default()
        .with_seed(2)
        .check(any_number, property)
        .unwrap_err();
    let again = Config::default()
        .with_seed(first.seed)
        .with_cases(1)
        .check(any_number, property)
        .unwrap_err();
    assert_eq!(again, first);
}

#[test]
fn case_seeds_wrap_past_u64_max_to_0() {
    let calls = Cell::new(0);
    let result = Config::default().with_seed(u64::MAX).check(
        |_| (),
        |&()| {
            calls.set(calls.get() + 1);
            calls.get() < 2
        },
    );
    assert_eq!(
        result,
        Err(failure(0, 1, (), (), "the property returned false", 0))
    );
}

#[test]
fn a_shrink_limit_of_0_reports_the_case_as_generated() {
    let result = Config::default()
        .with_shrink_limit(0)
        .check(any_number, |&number| number <= 1000);
    assert_eq!(
        result,
        Err(failure(
            0,
            0,
            11_091_344_671_253_066_419,
            11_091_344_671_253_066_419,
            "the property returned false",
            0
        ))
    );
}

// With 3 tries, the runner deletes the only choice (the case passes), then halves the number twice.
#[test]
fn shrinking_stops_after_the_shrink_limit() {
    let generated = Cell::new(0);
    let result = Config::default().with_shrink_limit(3).check(
        |source| {
            generated.set(generated.get() + 1);
            any_number(source)
        },
        |&number| number <= 1000,
    );
    assert_eq!(
        result,
        Err(failure(
            0,
            0,
            11_091_344_671_253_066_419,
            2_772_836_167_813_266_604,
            "the property returned false",
            2
        ))
    );
    assert_eq!(generated.get(), 4);
}

#[test]
fn shrinks_a_list_whose_length_comes_first() {
    let failure = Config::default()
        .check(list_with_length_first, |list| list.iter().all(|&n| n < 500))
        .unwrap_err();
    assert_eq!(failure.shrunk, [500]);
    assert_eq!(failure.message, "the property returned false");
}

// Deleting an element in front of the final 9 keeps the 9 last only if the length goes down too,
// which the shrinker tries when deleting the choices alone gives no smaller failing case.
#[test]
fn shrinks_a_list_whose_length_comes_first_by_deleting_elements_in_front() {
    let failure = Config::default()
        .check(
            |source| {
                let length = source.below(20);
                (0..length).map(|_| source.below(10)).collect::<Vec<_>>()
            },
            |list| list.last() != Some(&9),
        )
        .unwrap_err();
    assert_eq!(failure.shrunk, [9]);
    assert_eq!(failure.message, "the property returned false");
}

#[test]
fn shrinks_a_list_with_a_choice_before_each_element() {
    let failure = Config::default()
        .check(list_with_a_choice_per_element, |list| {
            list.iter().all(|&n| n < 500)
        })
        .unwrap_err();
    assert_eq!(failure.shrunk, [500]);
    assert_eq!(failure.message, "the property returned false");
}

#[test]
fn shrinks_a_list_to_the_shortest_that_fails() {
    let failure = Config::default()
        .check(list_with_a_choice_per_element, |list| list.len() < 3)
        .unwrap_err();
    assert_eq!(failure.shrunk, [0, 0, 0]);
}

#[test]
fn an_err_fails_with_its_message() {
    let failure = Config::default()
        .check(
            |source| source.below(10),
            |&number| {
                if number < 5 {
                    Ok(())
                } else {
                    Err(format!("{number} is too big"))
                }
            },
        )
        .unwrap_err();
    assert_eq!(failure.shrunk, 5);
    assert_eq!(
        failure.message,
        "the property returned an error: 5 is too big"
    );
}

#[test]
fn a_panic_with_a_str_fails_with_its_message() {
    let failure = Config::default()
        .check(
            |source| source.below(10),
            |&number| {
                assert!(number < 5, "too big");
                true
            },
        )
        .unwrap_err();
    assert_eq!(failure.shrunk, 5);
    assert_eq!(failure.message, "the property panicked: too big");
}

#[test]
fn a_panic_with_a_formatted_message_fails_with_it() {
    let failure = Config::default()
        .check(
            |source| source.below(10),
            |&number| {
                assert!(number < 5, "{number} is too big");
                true
            },
        )
        .unwrap_err();
    assert_eq!(failure.shrunk, 5);
    assert_eq!(failure.message, "the property panicked: 5 is too big");
}

#[test]
fn a_panic_with_a_payload_that_isnt_a_string_still_fails() {
    let failure = Config::default()
        .check(
            |source| source.below(10),
            |&number| {
                if number >= 5 {
                    panic::panic_any(number);
                }
                true
            },
        )
        .unwrap_err();
    assert_eq!(failure.shrunk, 5);
    assert_eq!(
        failure.message,
        "the property panicked with a payload that isn't a string"
    );
}

#[test]
#[should_panic(expected = "`Source::below` needs a bound above 0")]
fn a_generator_that_panics_panics_the_run() {
    let _ = Config::default().check(|source| source.below(0), |_| true);
}

#[test]
fn outcomes_convert_to_results() {
    assert_eq!(true.into_result(), Ok(()));
    assert_eq!(
        false.into_result(),
        Err(String::from("the property returned false"))
    );
    assert_eq!(Ok::<(), String>(()).into_result(), Ok(()));
    assert_eq!(
        Err::<(), _>("no").into_result(),
        Err(String::from("the property returned an error: no"))
    );
}

#[test]
fn a_failure_is_an_error() {
    let failure: Box<dyn std::error::Error> =
        Box::new(failure(3, 3, 2000, 1001, "the property returned false", 7));
    assert!(failure.source().is_none());
    assert_eq!(
        failure.to_string(),
        "property failed on case 3 (seed 3)\n\
         \x20 reason: the property returned false\n\
         \x20 shrunk case: 1001\n\
         \x20 original case: 2000\n\
         \x20 smaller failing cases found: 7\n\
         \x20 to rerun this case alone, add `.with_seed(3).with_cases(1)` to the run's `Config`"
    );
}
