//! Contains various backoff strategies.
//!
//! Strategies are defined as `Iterator<Item=Duration>`.

use std::iter::{self, Iterator};
use std::time::Duration;

use rand::prelude::thread_rng;
pub use rand::prelude::ThreadRng;

const MAX_RETRIES: u32 = 30;

/// Backoff type definition.
pub type Backoff = Iterator<Item = Duration>;

/// Creates a infinite stream of given `duration`
pub fn constant(duration: Duration) -> Constant {
    iter::repeat(duration)
}

/// Creates infinite stream of backoffs that keep the exponential growth from `start` until it
/// reaches `max`.
pub fn exponential(start: Duration, max: Duration) -> Exponential {
    assert!(
        start.as_secs() > 0,
        "start must be > 1s: {}",
        start.as_secs()
    );
    assert!(max.as_secs() > 0, "max must be > 1s: {}", max.as_secs());
    assert!(
        max >= start,
        "max must be greater then start: {} < {}",
        max.as_secs(),
        start.as_secs()
    );

    Exponential {
        start,
        max,
        attempt: 0,
    }
}

/// Creates infinite stream of backoffs that keep half of the exponential growth, and jitter
/// between 0 and that amount.
///
/// See https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/.
pub fn equal_jittered(start: Duration, max: Duration) -> EqualJittered {
    assert!(
        start.as_secs() > 0,
        "start must be > 1s: {}",
        start.as_secs()
    );
    assert!(max.as_secs() > 0, "max must be > 1s: {}", max.as_secs());
    assert!(
        max >= start,
        "max must be greater then start: {} < {}",
        max.as_secs(),
        start.as_secs()
    );

    EqualJittered {
        start,
        max,
        attempt: 0,
        rng: ThreadLocalGenRange,
    }
}

/// Creates infinite stream of backoffs that keep the exponential growth, and jitter
/// between 0 and that amount.
///
/// See https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/.
pub fn full_jittered(start: Duration, max: Duration) -> FullJittered {
    assert!(
        start.as_secs() > 0,
        "start must be > 1s: {}",
        start.as_secs()
    );
    assert!(max.as_secs() > 0, "max must be > 1s: {}", max.as_secs());
    assert!(
        max >= start,
        "max must be greater then start: {} < {}",
        max.as_secs(),
        start.as_secs()
    );

    FullJittered {
        start,
        max,
        attempt: 0,
        rng: ThreadLocalGenRange,
    }
}

/// Random generator.
pub trait GenRange {
    /// Generates a random value within range low and high.
    fn gen_range(&mut self, low: u64, high: u64) -> u64;
}

/// Thread local random generator, invokes `rand::thread_rng`.
#[derive(Debug, Clone)]
pub struct ThreadLocalGenRange;

impl GenRange for ThreadLocalGenRange {
    #[inline]
    fn gen_range(&mut self, low: u64, high: u64) -> u64 {
        use rand::Rng;
        thread_rng().gen_range(low, high)
    }
}

/// A type alias for constant backoff strategy, which is just iterator.
pub type Constant = iter::Repeat<Duration>;

/// An infinite stream of backoffs that keep the exponential growth from `start` until it
/// reaches `max`.
#[derive(Clone, Debug)]
pub struct Exponential {
    start: Duration,
    max: Duration,
    attempt: u32,
}

impl Iterator for Exponential {
    type Item = Duration;

    fn next(&mut self) -> Option<Self::Item> {
        let exp = exponential_backoff_seconds(self.attempt, self.start, self.max);

        if self.attempt < MAX_RETRIES {
            self.attempt += 1;
        }

        Some(Duration::from_secs(exp))
    }
}

/// An infinite stream of backoffs that keep half of the exponential growth, and jitter
/// between 0 and that amount.
///
/// See https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/.
#[derive(Clone, Debug)]
pub struct FullJittered<R = ThreadLocalGenRange> {
    start: Duration,
    max: Duration,
    attempt: u32,
    rng: R,
}

#[cfg(test)]
impl<R> FullJittered<R> {
    fn with_rng<T: GenRange>(self, rng: T) -> FullJittered<T> {
        FullJittered {
            rng,
            start: self.start,
            max: self.max,
            attempt: self.attempt,
        }
    }
}

impl<R: GenRange> Iterator for FullJittered<R> {
    type Item = Duration;

    fn next(&mut self) -> Option<Self::Item> {
        let seconds = self
            .rng
            .gen_range(self.start.as_secs(), self.max.as_secs() + 1);

        if self.attempt < MAX_RETRIES {
            self.attempt += 1;
        }

        Some(Duration::from_secs(seconds))
    }
}

/// Creates infinite stream of backoffs that keep the exponential growth, and jitter
/// between 0 and that amount.
///
/// See https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/.
#[derive(Clone, Debug)]
pub struct EqualJittered<R = ThreadLocalGenRange> {
    start: Duration,
    max: Duration,
    attempt: u32,
    rng: R,
}

#[cfg(test)]
impl<R> EqualJittered<R> {
    fn with_rng<T: GenRange>(self, rng: T) -> EqualJittered<T> {
        EqualJittered {
            rng,
            start: self.start,
            max: self.max,
            attempt: self.attempt,
        }
    }
}

impl<R: GenRange> Iterator for EqualJittered<R> {
    type Item = Duration;

    fn next(&mut self) -> Option<Self::Item> {
        let exp = exponential_backoff_seconds(self.attempt, self.start, self.max);
        let seconds = (exp / 2) + self.rng.gen_range(0, (exp / 2) + 1);

        if self.attempt < MAX_RETRIES {
            self.attempt += 1;
        }

        Some(Duration::from_secs(seconds))
    }
}

fn exponential_backoff_seconds(attempt: u32, base: Duration, max: Duration) -> u64 {
    ((1_u64 << attempt) * base.as_secs()).min(max.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prng::XorShiftRng;
    use rand::{RngCore, SeedableRng};

    const SEED: &'static [u8; 16] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 8, 7, 6, 5, 4, 3, 2];
    struct TestGenRage<T>(T);

    impl Default for TestGenRage<XorShiftRng> {
        fn default() -> Self {
            TestGenRage(XorShiftRng::from_seed(*SEED))
        }
    }

    impl<T: RngCore> GenRange for TestGenRage<T> {
        fn gen_range(&mut self, low: u64, high: u64) -> u64 {
            use rand::Rng;
            self.0.gen_range(low, high)
        }
    }

    #[test]
    fn exponential_growth() {
        let backoff = exponential(Duration::from_secs(10), Duration::from_secs(100));

        let actual = backoff.take(6).map(|it| it.as_secs()).collect::<Vec<_>>();
        let expected = vec![10, 20, 40, 80, 100, 100];
        assert_eq!(expected, actual);
    }

    #[test]
    fn full_jittered_growth() {
        let backoff = full_jittered(Duration::from_secs(10), Duration::from_secs(100))
            .with_rng(TestGenRage::default());

        let actual = backoff.take(10).map(|it| it.as_secs()).collect::<Vec<_>>();
        let expected = vec![26, 13, 69, 32, 61, 69, 55, 46, 92, 22];
        assert_eq!(expected, actual);
    }

    #[test]
    fn equal_jittered_growth() {
        let backoff = equal_jittered(Duration::from_secs(5), Duration::from_secs(300))
            .with_rng(TestGenRage::default());

        let actual = backoff.take(10).map(|it| it.as_secs()).collect::<Vec<_>>();
        let expected = vec![2, 5, 10, 37, 63, 133, 225, 153, 216, 170];
        assert_eq!(expected, actual)
    }

    #[test]
    fn constant_growth() {
        let backoff = constant(Duration::from_secs(3));

        let actual = backoff.take(3).map(|it| it.as_secs()).collect::<Vec<_>>();
        let expected = vec![3, 3, 3];
        assert_eq!(expected, actual);
    }
}
