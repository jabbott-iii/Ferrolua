//! Mutation: each run of the fuzzer takes an input from the target's corpus and changes it in one
//! to [`MAX_EDITS`] random ways.
//!
//! Run `n` of a campaign from seed `s` draws every random choice from an
//! [`Xoshiro256StarStar`] seeded with `s + n`, wrapping past `u64::MAX` to 0, so a run's seed
//! makes its input again on every target. The edits are the three that the quality document
//! names:
//!
//! - **Flip:** flip one bit of one byte.
//! - **Insert:** insert one random byte anywhere, the end included.
//! - **Splice:** keep the input up to a random point, and replace the rest with the end of an
//!   input from the corpus, taken from a random point. The two can be the same input.

use ferrolua_testkit::Xoshiro256StarStar;

/// The largest number of edits one run makes.
pub(crate) const MAX_EDITS: u64 = 8;

/// Makes the input of the run whose seed is `seed`, from `corpus`, which must hold at least one
/// input.
pub(crate) fn mutate(corpus: &[Vec<u8>], seed: u64) -> Vec<u8> {
    let mut random = Xoshiro256StarStar::from_seed(seed);
    let mut input = corpus[index(&mut random, corpus.len())].clone();
    for _ in 0..=random.below(MAX_EDITS) {
        match random.below(3) {
            0 => {
                if !input.is_empty() {
                    let at = index(&mut random, input.len());
                    flip(&mut input, at, bit(&mut random));
                }
            }
            1 => {
                let at = index(&mut random, input.len() + 1);
                insert(&mut input, at, byte(&mut random));
            }
            _ => {
                let other = &corpus[index(&mut random, corpus.len())];
                let head = index(&mut random, input.len() + 1);
                let tail = index(&mut random, other.len() + 1);
                splice(&mut input, head, other, tail);
            }
        }
    }
    input
}

/// Flips bit `bit` of byte `at`.
fn flip(input: &mut [u8], at: usize, bit: u32) {
    input[at] ^= 1 << bit;
}

/// Inserts `byte` before byte `at`, or at the end when `at` is the length.
fn insert(input: &mut Vec<u8>, at: usize, byte: u8) {
    input.insert(at, byte);
}

/// Keeps the first `head` bytes of `input`, and appends the bytes of `other` from `tail` on.
fn splice(input: &mut Vec<u8>, head: usize, other: &[u8], tail: usize) {
    input.truncate(head);
    input.extend_from_slice(&other[tail..]);
}

/// Chooses a position below `length`, which must be above 0.
fn index(random: &mut Xoshiro256StarStar, length: usize) -> usize {
    let length = u64::try_from(length).expect("a length fits in 64 bits on every target");
    usize::try_from(random.below(length)).expect("a number below a length fits in `usize`")
}

/// Chooses a bit of a byte.
fn bit(random: &mut Xoshiro256StarStar) -> u32 {
    u32::try_from(random.below(8)).expect("a number below 8 fits in `u32`")
}

/// Chooses a byte.
fn byte(random: &mut Xoshiro256StarStar) -> u8 {
    random.next_u64().to_le_bytes()[0]
}

#[cfg(test)]
mod tests {
    use super::{MAX_EDITS, flip, insert, mutate, splice};

    #[test]
    fn flips_one_bit_of_one_byte() {
        let mut input = b"FUZ".to_vec();
        flip(&mut input, 1, 5);
        assert_eq!(input, b"FuZ");
    }

    #[test]
    fn inserts_a_byte_anywhere_the_end_included() {
        let mut input = b"FUZ".to_vec();
        insert(&mut input, 0, b'>');
        insert(&mut input, 4, b'Z');
        assert_eq!(input, b">FUZZ");
    }

    #[test]
    fn splices_the_end_of_another_input_after_the_start_of_this_one() {
        let mut input = b"FUZ".to_vec();
        splice(&mut input, 3, b"FUZ", 2);
        assert_eq!(input, b"FUZZ");
        splice(&mut input, 1, b"", 0);
        assert_eq!(input, b"F");
    }

    #[test]
    fn makes_the_same_input_from_the_same_seed() {
        let corpus = [b"FUZ".to_vec(), b"a longer input".to_vec()];
        for seed in 0..100 {
            assert_eq!(mutate(&corpus, seed), mutate(&corpus, seed));
        }
    }

    #[test]
    fn grows_an_empty_input_by_at_most_one_byte_an_edit() {
        let corpus = [Vec::new()];
        for seed in 0..1000 {
            let input = mutate(&corpus, seed);
            assert!(
                u64::try_from(input.len()).unwrap() <= MAX_EDITS,
                "{input:?}"
            );
        }
    }
}
