//! This module contains generic backoff utlities to be used with the retry
//! layer.
//!
//! The [`Backoff`] trait is a generic way to represent backoffs that can use
//! any timer type.
//!
//! [`ExponentialBackoff`] implements [`Backoff`] and provides a batteries
//! included exponential backoff and jitter strategy.

use rand::Rng;
use rand::{rngs::SmallRng, thread_rng, SeedableRng};
use std::fmt::Display;
use std::future::Future;
use std::time::Duration;
use tokio::time;

/// A backoff trait where a single mutable reference represents a single
/// backoff session. Implementors must also implement [`Clone`] which will
/// reset the backoff back to the default state for the next session.
pub trait Backoff: Clone {
    /// The future associated with each backoff. This usually will be some sort
    /// of timer.
    type Future: Future<Output = ()>;

    /// Initiate the next backoff in the sequence.
    fn next_backoff(&mut self) -> Self::Future;
}

/// A jittered exponential backoff strategy.
#[derive(Debug)]
pub struct ExponentialBackoff<R = SmallRng> {
    /// The minimum amount of time to wait before resuming an operation.
    min: time::Duration,

    /// The maximum amount of time to wait before resuming an operation.
    max: time::Duration,

    /// The ratio of the base timeout that may be randomly added to a backoff.
    ///
    /// Must be greater than or equal to 0.0.
    jitter: f64,

    rng: R,
    iterations: u32,
}

impl<R> ExponentialBackoff<R>
where
    R: Rng,
{
    /// Create a new `ExponentialBackoff`.
    ///
    /// # Error
    ///
    /// Returns a config validation error if:
    /// - `min` > `max`
    /// - `max` > 0
    /// - `jitter` >= `0.0`
    /// - `jitter` < `100.0`
    /// - `jitter` is finite
    pub fn new(
        min: time::Duration,
        max: time::Duration,
        jitter: f64,
        rng: R,
    ) -> Result<Self, InvalidBackoff> {
        if min > max {
            return Err(InvalidBackoff("maximum must not be less than minimum"));
        }
        if max == time::Duration::from_millis(0) {
            return Err(InvalidBackoff("maximum must be non-zero"));
        }
        if jitter < 0.0 {
            return Err(InvalidBackoff("jitter must not be negative"));
        }
        if jitter > 100.0 {
            return Err(InvalidBackoff("jitter must not be greater than 100"));
        }
        if !jitter.is_finite() {
            return Err(InvalidBackoff("jitter must be finite"));
        }

        Ok(ExponentialBackoff {
            min,
            max,
            jitter,
            rng,
            iterations: 0,
        })
    }

    fn base(&self) -> time::Duration {
        debug_assert!(
            self.min <= self.max,
            "maximum backoff must not be less than minimum backoff"
        );
        debug_assert!(
            self.max > time::Duration::from_millis(0),
            "Maximum backoff must be non-zero"
        );
        self.min
            .checked_mul(2_u32.saturating_pow(self.iterations))
            .unwrap_or(self.max)
            .min(self.max)
    }

    /// Returns a random, uniform duration on `[0, base*self.jitter]` no greater
    /// than `self.max`.
    fn jitter(&mut self, base: time::Duration) -> time::Duration {
        if self.jitter == 0.0 {
            time::Duration::default()
        } else {
            let jitter_factor = self.rng.gen::<f64>();
            debug_assert!(
                jitter_factor > 0.0,
                "rng returns values between 0.0 and 1.0"
            );
            let rand_jitter = jitter_factor * self.jitter;
            let secs = (base.as_secs() as f64) * rand_jitter;
            let nanos = (base.subsec_nanos() as f64) * rand_jitter;
            let remaining = self.max - base;
            time::Duration::new(secs as u64, nanos as u32).min(remaining)
        }
    }
}

impl Backoff for ExponentialBackoff {
    type Future = tokio::time::Sleep;

    fn next_backoff(&mut self) -> Self::Future {
        let base = self.base();
        let next = base + self.jitter(base);

        self.iterations += 1;

        tokio::time::sleep(next)
    }
}

impl<R> Clone for ExponentialBackoff<R>
where
    R: Clone,
{
    fn clone(&self) -> Self {
        Self {
            min: self.min,
            max: self.max,
            jitter: self.jitter,
            rng: self.rng.clone(),
            iterations: 0,
        }
    }
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        let rng = SmallRng::from_rng(&mut thread_rng()).expect("RNG must be valid");
        ExponentialBackoff::new(
            Duration::from_millis(50),
            Duration::from_millis(u64::MAX),
            0.99,
            rng,
        )
        .expect("Unable to create ExponentialBackoff")
    }
}

/// Backoff validation error.
#[derive(Clone, Debug)]
pub struct InvalidBackoff(&'static str);

impl Display for InvalidBackoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for InvalidBackoff {}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::*;

    quickcheck! {
        fn backoff_base_first(min_ms: u64, max_ms: u64) -> TestResult {
            let min = time::Duration::from_millis(min_ms);
            let max = time::Duration::from_millis(max_ms);
            let rng = SmallRng::from_rng(&mut thread_rng()).expect("RNG must be valid");
            let backoff = match ExponentialBackoff::new(min, max, 0.0, rng) {
                Err(_) => return TestResult::discard(),
                Ok(backoff) => backoff,
            };
            let delay = backoff.base();
            TestResult::from_bool(min == delay)
        }

        fn backoff_base(min_ms: u64, max_ms: u64, iterations: u32) -> TestResult {
            let min = time::Duration::from_millis(min_ms);
            let max = time::Duration::from_millis(max_ms);
            let rng = SmallRng::from_rng(&mut thread_rng()).expect("RNG must be valid");
            let mut backoff = match ExponentialBackoff::new(min, max, 0.0, rng) {
                Err(_) => return TestResult::discard(),
                Ok(backoff) => backoff,
            };
            backoff.iterations = iterations;
            let delay = backoff.base();
            TestResult::from_bool(min <= delay && delay <= max)
        }

        fn backoff_jitter(base_ms: u64, max_ms: u64, jitter: f64) -> TestResult {
            let base = time::Duration::from_millis(base_ms);
            let max = time::Duration::from_millis(max_ms);
            let rng = SmallRng::from_rng(&mut thread_rng()).expect("RNG must be valid");
            let mut backoff = match ExponentialBackoff::new(base, max, jitter, rng) {
                Err(_) => return TestResult::discard(),
                Ok(backoff) => backoff,
            };

            let j = backoff.jitter(base);
            if jitter == 0.0 || base_ms == 0 || max_ms == base_ms {
                TestResult::from_bool(j == time::Duration::default())
            } else {
                TestResult::from_bool(j > time::Duration::default())
            }
        }
    }
}
