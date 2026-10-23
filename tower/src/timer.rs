//! Pluggable timers for the time-based middleware.
//!
//! Middleware that needs to sleep is generic over a [`Timer`], and uses
//! [`TokioTimer`] unless another timer is passed in. This lets the same
//! middleware run on runtimes other than Tokio.

use std::{
    future::Future,
    pin::Pin,
    time::{Duration, Instant},
};

/// A source of sleeps and the current time.
pub trait Timer {
    /// The future returned by [`sleep`](Timer::sleep) and
    /// [`sleep_until`](Timer::sleep_until).
    type Sleep: Future<Output = ()>;

    /// Returns a future that completes after `duration` has elapsed.
    fn sleep(&self, duration: Duration) -> Self::Sleep;

    /// Returns a future that completes at `deadline`.
    fn sleep_until(&self, deadline: Instant) -> Self::Sleep;

    /// Returns the current time.
    fn now(&self) -> Instant {
        Instant::now()
    }

    /// Moves the deadline of an existing sleep.
    ///
    /// The default replaces the sleep with a new one.
    fn reset(&self, mut sleep: Pin<&mut Self::Sleep>, deadline: Instant) {
        sleep.set(self.sleep_until(deadline));
    }
}

/// A [`Timer`] backed by [`tokio::time`].
#[derive(Clone, Copy, Debug, Default)]
pub struct TokioTimer;

impl Timer for TokioTimer {
    type Sleep = tokio::time::Sleep;

    fn sleep(&self, duration: Duration) -> Self::Sleep {
        tokio::time::sleep(duration)
    }

    fn sleep_until(&self, deadline: Instant) -> Self::Sleep {
        tokio::time::sleep_until(deadline.into())
    }

    fn now(&self) -> Instant {
        tokio::time::Instant::now().into_std()
    }

    fn reset(&self, sleep: Pin<&mut Self::Sleep>, deadline: Instant) {
        sleep.reset(deadline.into());
    }
}
