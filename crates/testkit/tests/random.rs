//! Tests that `SplitMix64` and xoshiro256** give the outputs of their authors' reference code.
//!
//! The expected numbers come from `splitmix64.c` and `xoshiro256starstar.c`, the reference code
//! that David Blackman and Sebastiano Vigna publish at <https://prng.di.unimi.it/>. On 2026-11-25
//! their `next` functions were compiled with GCC 13.3.0 on x86-64 Linux and run from the seeds and
//! states each test names. The expected results of `below` come from a C version of Lemire's
//! method run on that code's outputs, or from the outputs by hand where a test says how.

use ferrolua_testkit::{SplitMix64, Xoshiro256StarStar};

/// Calls `next` `N` times and returns the results in order.
fn outputs<const N: usize>(mut next: impl FnMut() -> u64) -> [u64; N] {
    std::array::from_fn(|_| next())
}

/// Builds a generator from a state that isn't all zeros.
fn from_state(state: [u64; 4]) -> Xoshiro256StarStar {
    Xoshiro256StarStar::from_state(state).expect("the tests pass states that aren't all zeros")
}

#[test]
fn splitmix64_from_seed_0_gives_the_reference_outputs() {
    let mut generator = SplitMix64::new(0);
    assert_eq!(
        outputs(|| generator.next_u64()),
        [
            16_294_208_416_658_607_535,
            7_960_286_522_194_355_700,
            487_617_019_471_545_679,
            17_909_611_376_780_542_444,
            1_961_750_202_426_094_747,
        ]
    );
}

#[test]
fn splitmix64_from_seed_1234567_gives_the_reference_outputs() {
    let mut generator = SplitMix64::new(1_234_567);
    assert_eq!(
        outputs(|| generator.next_u64()),
        [
            6_457_827_717_110_365_317,
            3_203_168_211_198_807_973,
            9_817_491_932_198_370_423,
            4_593_380_528_125_082_431,
            16_408_922_859_458_223_821,
        ]
    );
}

#[test]
fn xoshiro_from_state_1_2_3_4_gives_the_reference_outputs() {
    let mut generator = from_state([1, 2, 3, 4]);
    assert_eq!(
        outputs(|| generator.next_u64()),
        [
            11_520,
            0,
            1_509_978_240,
            1_215_971_899_390_074_240,
            1_216_172_134_540_287_360,
            607_988_272_756_665_600,
            16_172_922_978_634_559_625,
            8_476_171_486_693_032_832,
            10_595_114_339_597_558_777,
            2_904_607_092_377_533_576,
        ]
    );
}

#[test]
fn xoshiro_from_a_state_with_one_bit_set_gives_the_reference_outputs() {
    let mut generator = from_state([0, 0, 0, 1]);
    assert_eq!(
        outputs(|| generator.next_u64()),
        [0, 0, 5_760, 202_661_983_231_678_080]
    );
}

#[test]
fn xoshiro_rejects_a_state_of_all_zeros() {
    assert_eq!(Xoshiro256StarStar::from_state([0; 4]), None);
}

#[test]
fn xoshiro_from_seed_0_gives_the_reference_outputs_seeded_by_splitmix64() {
    let mut generator = Xoshiro256StarStar::from_seed(0);
    assert_eq!(
        outputs(|| generator.next_u64()),
        [
            11_091_344_671_253_066_420,
            13_793_997_310_169_335_082,
            1_900_383_378_846_508_768,
            7_684_712_102_626_143_532,
            13_521_403_990_117_723_737,
            18_442_103_541_295_991_498,
            7_788_427_924_976_520_344,
            9_881_088_229_871_127_103,
            15_781_505_947_799_885_617,
            16_949_938_600_482_740_797,
        ]
    );
}

#[test]
fn xoshiro_from_seed_starts_from_four_splitmix64_outputs() {
    let mut expander = SplitMix64::new(41);
    let state = outputs(|| expander.next_u64());
    assert_eq!(Xoshiro256StarStar::from_seed(41), from_state(state));
}

#[test]
fn the_same_seed_gives_the_same_sequence() {
    let mut first = Xoshiro256StarStar::from_seed(7);
    let mut second = Xoshiro256StarStar::from_seed(7);
    let first: [u64; 1000] = outputs(|| first.next_u64());
    let second: [u64; 1000] = outputs(|| second.next_u64());
    assert_eq!(first, second);
}

#[test]
fn neighboring_seeds_give_different_outputs() {
    let mut zero = Xoshiro256StarStar::from_seed(0);
    let mut one = Xoshiro256StarStar::from_seed(1);
    assert_ne!(zero.next_u64(), one.next_u64());
}

#[test]
fn a_clone_continues_the_same_sequence() {
    let mut original = Xoshiro256StarStar::from_seed(3);
    original.next_u64();
    let mut copy = original.clone();
    assert_eq!(
        outputs::<5>(|| copy.next_u64()),
        outputs::<5>(|| original.next_u64())
    );

    let mut expander = SplitMix64::new(3);
    let mut expander_copy = expander.clone();
    assert_eq!(expander_copy.next_u64(), expander.next_u64());
}

// For a bound of `u64::MAX`, an output `x` above 0 gives `x - 1`, and an output of 0 is drawn
// again, because 2^64 mod `u64::MAX` is 1, so a low half of 0 would favor one result.
#[test]
fn below_draws_again_when_the_low_bits_would_favor_some_results() {
    let mut generator = from_state([1, 2, 3, 4]);
    assert_eq!(generator.below(u64::MAX), 11_519);
    // The second reference output, 0, is drawn again, so this call uses the third.
    assert_eq!(generator.below(u64::MAX), 1_509_978_239);
    assert_eq!(generator.next_u64(), 1_215_971_899_390_074_240);
}

// For a bound of 2^63, every low half is fair, so each result is the output shifted right by 1,
// and an output of 0 gives 0 instead of a new draw.
#[test]
fn below_a_power_of_two_keeps_the_high_bits_of_each_output() {
    let mut generator = from_state([1, 2, 3, 4]);
    assert_eq!(
        outputs(|| generator.below(1 << 63)),
        [5_760, 0, 754_989_120]
    );
}

#[test]
fn below_1_always_gives_0_and_uses_one_output() {
    let mut generator = Xoshiro256StarStar::from_seed(5);
    let mut reference = generator.clone();
    let results: [u64; 100] = outputs(|| generator.below(1));
    assert_eq!(results, [0; 100]);
    for _ in 0..100 {
        reference.next_u64();
    }
    assert_eq!(generator, reference);
}

#[test]
fn below_10_from_seed_0_gives_the_counts_of_the_reference_code() {
    let mut generator = Xoshiro256StarStar::from_seed(0);
    let mut counts = [0_u32; 10];
    for _ in 0..10_000 {
        let number = generator.below(10);
        let slot = usize::try_from(number).unwrap();
        counts[slot] += 1;
    }
    assert_eq!(
        counts,
        [1001, 1016, 959, 989, 1020, 980, 997, 1035, 1000, 1003]
    );
}

#[test]
#[should_panic(expected = "`below` needs a bound above 0")]
fn below_0_panics() {
    Xoshiro256StarStar::from_seed(0).below(0);
}

#[test]
fn debug_shows_the_state() {
    assert_eq!(
        format!("{:?}", SplitMix64::new(9)),
        "SplitMix64 { state: 9 }"
    );
    assert_eq!(
        format!("{:?}", from_state([1, 2, 3, 4])),
        "Xoshiro256StarStar { state: [1, 2, 3, 4] }"
    );
}
