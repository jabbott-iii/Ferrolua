//! Seeded random numbers: [`SplitMix64`] to expand a 64-bit seed, and [`Xoshiro256StarStar`] to
//! generate from the expanded state.
//!
//! Both follow the reference C code that their authors, David Blackman and Sebastiano Vigna,
//! publish at <https://prng.di.unimi.it/>, and this crate's tests compare their output with what
//! that code prints. Both use only wrapping 64-bit integer arithmetic, which Rust defines the same
//! way on every target, so a seed gives the same numbers everywhere.

/// SplitMix64, a generator with 64 bits of state, which [`Xoshiro256StarStar::from_seed`] uses to
/// expand a 64-bit seed into a 256-bit state.
///
/// Every seed is valid, zero included. The xoshiro authors suggest filling a xoshiro state from
/// SplitMix64's output when the seed has 64 bits, and four consecutive outputs never repeat a
/// value, so they never fill the state with zeros.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Makes a generator that starts from `seed`.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Returns the next output and advances the state.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// xoshiro256**, the generator behind every random choice in Lunaferra's tests: 256 bits of state,
/// 64-bit outputs, and a period of 2^256 − 1.
///
/// It isn't a cryptographic generator, so never use it for keys or other secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xoshiro256StarStar {
    state: [u64; 4],
}

impl Xoshiro256StarStar {
    /// Makes a generator from a 64-bit seed, filling its state with the first four outputs of a
    /// [`SplitMix64`] that starts from `seed`.
    pub fn from_seed(seed: u64) -> Self {
        let mut expander = SplitMix64::new(seed);
        let state = [
            expander.next_u64(),
            expander.next_u64(),
            expander.next_u64(),
            expander.next_u64(),
        ];
        Self { state }
    }

    /// Makes a generator that starts from `state` exactly, as the reference code does when its
    /// state array holds `state`.
    ///
    /// Returns `None` when every word of `state` is zero, because xoshiro256** would then return
    /// zero forever.
    pub fn from_state(state: [u64; 4]) -> Option<Self> {
        if state == [0; 4] {
            None
        } else {
            Some(Self { state })
        }
    }

    /// Returns the next output and advances the state.
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.state;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// Returns a number from 0 up to but not including `bound`, with every number in that range
    /// equally likely.
    ///
    /// This is Daniel Lemire's method: multiply an output by `bound` and keep the high 64 bits of
    /// the 128-bit product. A few values of the low 64 bits would make some results more likely
    /// than others, so when the low bits take one of those values, it draws another output. The
    /// chance of drawing again is below `bound` / 2^64.
    ///
    /// # Panics
    ///
    /// Panics if `bound` is 0, because no number is below 0.
    pub fn below(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "`below` needs a bound above 0");
        // 2^64 mod `bound`: this many values of the low bits would favor some results.
        let rejected = bound.wrapping_neg() % bound;
        loop {
            let product = u128::from(self.next_u64()) * u128::from(bound);
            let [low, high] = split(product);
            if low >= rejected {
                return high;
            }
        }
    }
}

/// Splits a 128-bit number into its low and high 64 bits.
fn split(number: u128) -> [u64; 2] {
    let low = u64::try_from(number & u128::from(u64::MAX)).expect("the low 64 bits fit in u64");
    let high = u64::try_from(number >> 64).expect("a u128 shifted right by 64 fits in u64");
    [low, high]
}
