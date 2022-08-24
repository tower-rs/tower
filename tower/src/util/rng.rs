//! [PRNG] utilities for tower middleware.
//!
//! This module provides a generic [`Rng`] trait and a [`HasherRng`] that
//! implements the trait based on [`RandomState`] or any other [`Hasher`].
//!
//! These utlities replace tower's internal usage of `rand` with these smaller,
//! more lightweight methods. Most of the implementations are extracted from
//! their corresponding `rand` implementations.
//!
//! [PRNG]: https://en.wikipedia.org/wiki/Pseudorandom_number_generator

use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    ops::Range,
};

/// A simple [PRNG] trait for use within tower middleware.
///
/// [PRNG]: https://en.wikipedia.org/wiki/Pseudorandom_number_generator
pub trait Rng {
    /// Generate a random [`u64`].
    fn next_u64(&mut self) -> u64;

    /// Generate a random [`f64`] between `[0, 1)`.
    fn next_f64(&mut self) -> f64 {
        // Borrowed from:
        // https://github.com/rust-random/rand/blob/master/src/distributions/float.rs#L106
        let float_size = std::mem::size_of::<f64>() as u32 * 8;
        let precison = 52 + 1;
        let scale = 1.0 / ((1u64 << precison) as f64);

        let value = self.next_u64();
        let value = value >> (float_size - precison);

        scale * value as f64
    }

    /// Randomly pick a value within the range.
    ///
    /// # Panic
    ///
    /// - If start < end this will panic in debug mode.
    fn next_range(&mut self, range: Range<u64>) -> u64 {
        debug_assert!(
            range.start < range.end,
            "The range start must be smaller than the end"
        );
        let start = range.start;
        let end = range.end;

        let range = end - start;

        let n = self.next_u64();

        (n % range) + start
    }
}

impl<R: Rng + ?Sized> Rng for Box<R> {
    fn next_u64(&mut self) -> u64 {
        (**self).next_u64()
    }
}

/// A [`Rng`] implementation that uses a [`Hasher`] to generate the random
/// values. The implementation uses an internal counter to pass to the hasher
/// for each iteration of [`Rng::next_u64`].
///
/// # Default
///
/// This hasher has a default type of [`RandomState`] which just uses the
/// libstd method of getting a random u64.
#[derive(Debug)]
pub struct HasherRng<H = RandomState> {
    hasher: H,
    counter: u64,
}

impl HasherRng {
    /// Create a new default [`HasherRng`].
    pub fn new() -> Self {
        HasherRng::default()
    }
}

impl Default for HasherRng {
    fn default() -> Self {
        HasherRng::with_hasher(RandomState::default())
    }
}

impl<H> HasherRng<H> {
    /// Create a new [`HasherRng`] with the provided hasher.
    pub fn with_hasher(hasher: H) -> Self {
        HasherRng { hasher, counter: 0 }
    }
}

impl<H> Rng for HasherRng<H>
where
    H: BuildHasher,
{
    fn next_u64(&mut self) -> u64 {
        let mut hasher = self.hasher.build_hasher();
        hasher.write_u64(self.counter);
        self.counter = self.counter.wrapping_add(1);
        hasher.finish()
    }
}

/// An inplace sampler borrowed from the Rand implementation for use internally
/// for the balance middleware.
/// ref: https://github.com/rust-random/rand/blob/b73640705d6714509f8ceccc49e8df996fa19f51/src/seq/index.rs#L425
///
/// Docs from rand:
///
/// Randomly sample exactly `amount` indices from `0..length`, using an inplace
/// partial Fisher-Yates method.
/// Sample an amount of indices using an inplace partial fisher yates method.
///
/// This allocates the entire `length` of indices and randomizes only the first `amount`.
/// It then truncates to `amount` and returns.
///
/// This method is not appropriate for large `length` and potentially uses a lot
/// of memory; because of this we only implement for `u32` index (which improves
/// performance in all cases).
///
/// Set-up is `O(length)` time and memory and shuffling is `O(amount)` time.
pub(crate) fn sample_inplace<R: Rng>(rng: &mut R, length: u32, amount: u32) -> Vec<u32> {
    debug_assert!(amount <= length);
    let mut indices: Vec<u32> = Vec::with_capacity(length as usize);
    indices.extend(0..length);
    for i in 0..amount {
        let j: u64 = rng.next_range(i as u64..length as u64);
        indices.swap(i as usize, j as usize);
    }
    indices.truncate(amount as usize);
    debug_assert_eq!(indices.len(), amount as usize);
    indices
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::*;

    quickcheck! {
        fn next_f64(counter: u64) -> TestResult {
            let mut rng = HasherRng::default();
            rng.counter = counter;
            let n = rng.next_f64();

            TestResult::from_bool(n < 1.0 && n >= 0.0)
        }

        fn next_range(counter: u64, range: Range<u64>) -> TestResult {
            if  range.start >= range.end{
                return TestResult::discard();
            }

            let mut rng = HasherRng::default();
            rng.counter = counter;

            let n = rng.next_range(range.clone());

            TestResult::from_bool(n >= range.start && (n < range.end || range.start == range.end))
        }

        fn sample_inplace(counter: u64, length: u32, amount: u32) -> TestResult {
            if amount > length || length > 256 || amount > 32  {
                return TestResult::discard();
            }

            let mut rng = HasherRng::default();
            rng.counter = counter;

            let indxs = super::sample_inplace(&mut rng, length, amount);

            for indx in indxs {
                if indx > length {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }
    }

    #[test]
    fn sample_inplace_boundaries() {
        let mut r = HasherRng::default();

        assert_eq!(super::sample_inplace(&mut r, 0, 0).len(), 0);
        assert_eq!(super::sample_inplace(&mut r, 1, 0).len(), 0);
        assert_eq!(super::sample_inplace(&mut r, 1, 1), vec![0]);
    }
}
