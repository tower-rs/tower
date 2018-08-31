use std::time::{Instant, Duration};
use std::iter::Iterator;

use tokio_timer::clock;

use super::backoff;
use super::ema::Ema;

static DEFAULT_BACKOFF: Duration = Duration::from_secs(300);

const SUCCESS: f64 = 1.0;
const FAILURE: f64 = 0.0;
const MILLIS_PER_SECOND: u64 = 1_000;
const DEFAULT_SUCCESS_RATE_THRESHOLD: f64 = 0.8;
const DEFAULT_SUCCESS_RATE_WINDOW_SECONDS: u64 = 30;
const DEFAULT_CONSECUTIVE_FAILURES: u32 = 5;

/// A `FailureAccrualPolicy` is uses to determine whether to mark an endpoint dead upon a request
/// failure.
pub trait FailureAccrualPolicy {
    /// Invoked when a request is successful.
    fn record_success(&mut self);

    /// Invoked when a non-probing request fails.  If it returns `Some(Duration)`,
    /// the endpoint will mark as the dead for the specified `Duration`.
    fn mark_dead_on_failure(&mut self) -> Option<Duration>;

    /// Invoked  when an endpoint is revived after probing. Used to reset any history.
    fn revived(&mut self);

    /// Creates a `FailureAccrualPolicy` which uses both `self` and `rhs`.
    fn or_else<R>(self, rhs: R) -> OrElse<Self, R>
    where
        Self: Sized
    {
        OrElse {
            left: self,
            right: rhs,
        }
    }
}

/// Returns a policy based on an exponentially-weighted moving average success
/// rate over a time window. A moving average is used so the success rate
/// calculation is biased towards more recent requests.
///
/// If the computed weighted success rate is less than the required success rate,
/// `mark_dead_on_failure` will return `Some(Duration)`.
///
/// See `ema::Ema` for how the success rate is computed.
///
/// * `required_success_rate` - a success rate that must be met.
/// * `window` - window over which the success rate is tracked.  `mark_dead_on_failure` will return
///   None, until we get requests for a duration of at least `window`.
/// * `backoff` - stream of durations to use for the next duration
///   returned from `mark_dead_on_failure`
///
/// # Panics
///
/// When `required_success_rate` isn't in `[0.0, 0.1]` interval.
pub fn success_rate<B>(required_success_rate: f64, window: Duration, backoff: B) -> SuccessRate<B>
where
    B: Iterator<Item=Duration> + Clone
{
    assert!(
        required_success_rate >= 0.0 && required_success_rate <= 1.0,
        "required_success_rate must be [0, 1]: {}", required_success_rate
    );

    let window_millis = window.as_secs() * MILLIS_PER_SECOND;

    SuccessRate {
        required_success_rate,
        ema: Ema::new(window_millis),
        now: clock::now(),
        window_millis,
        backoff,
    }
}

/// A policy based on a maximum number of consecutive failures. If `num_failures`
/// occur consecutively, `mark_dead_on_failure` will return a Some(Duration) to
/// mark an endpoint dead for.
///
/// * `num_failures` - number of consecutive failures.
/// * `backoff` - stream of durations to use for the next duration
///   returned from `mark_dead_on_failure`
pub fn consecutive_failures<B>(num_failures: u32, backoff: B) -> ConsecutiveFailures<B>
where
    B: Iterator<Item=Duration> + Clone
{
    ConsecutiveFailures {
        num_failures,
        consecutive_failures: 0,
        backoff,
    }
}

impl Default for SuccessRate<backoff::EqualJittered> {
    fn default() -> Self {
        success_rate(
            DEFAULT_SUCCESS_RATE_THRESHOLD,
            Duration::from_secs(DEFAULT_SUCCESS_RATE_WINDOW_SECONDS),
            Default::default(),
        )
    }
}

impl Default for ConsecutiveFailures<backoff::EqualJittered> {
    fn default() -> Self {
        consecutive_failures(DEFAULT_CONSECUTIVE_FAILURES, Default::default())
    }
}

#[derive(Debug)]
pub struct SuccessRate<B> {
    required_success_rate: f64,
    ema: Ema,
    now: Instant,
    window_millis: u64,
    backoff: B,
}

impl<B> SuccessRate<B>
where
    B: Clone
{
    /// Returns seconds since instance was created.
    fn elapsed_millis(&self) -> u64 {
        let diff = clock::now() - self.now;
        (diff.as_secs() * MILLIS_PER_SECOND) + (diff.subsec_millis() as u64)
    }

    /// We can trigger failure accrual if the `window` has passed, success rate is below
    /// `required_success_rate`.
    fn can_remove(&self, success_rate: f64) -> bool {
        self.elapsed_millis() >= self.window_millis && success_rate < self.required_success_rate
    }
}

impl<B> FailureAccrualPolicy for SuccessRate<B>
where
    B: Iterator<Item=Duration> + Clone
{
    #[inline]
    fn record_success(&mut self) {
        let timestamp = self.elapsed_millis();
        self.ema.update(timestamp, SUCCESS);
    }

    #[inline]
    fn mark_dead_on_failure(&mut self) -> Option<Duration> {
        let timestamp = self.elapsed_millis();
        let success_rate = self.ema.update(timestamp, FAILURE);

        if self.can_remove(success_rate) {
            let duration = self.backoff.next().unwrap_or(DEFAULT_BACKOFF);
            Some(duration)
        } else {
            None
        }
    }

    #[inline]
    fn revived(&mut self) {
        self.now = clock::now();
        self.ema.reset();
    }
}

#[derive(Debug)]
pub struct ConsecutiveFailures<B> {
    num_failures: u32,
    consecutive_failures: u32,
    backoff: B,
}

impl<B> FailureAccrualPolicy for ConsecutiveFailures<B>
where
    B: Iterator<Item=Duration> + Clone
{
    #[inline]
    fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    #[inline]
    fn mark_dead_on_failure(&mut self) -> Option<Duration> {
        self.consecutive_failures += 1;

        if self.consecutive_failures >= self.num_failures {
            let duration = self.backoff.next().unwrap_or(DEFAULT_BACKOFF);
            Some(duration)
        } else {
            None
        }
    }

    #[inline]
    fn revived(&mut self) {
        self.consecutive_failures = 0;
    }
}

#[derive(Debug)]
pub struct OrElse<L, R> {
    left: L,
    right: R,
}

impl<L, R> FailureAccrualPolicy for OrElse<L, R>
where
    L: FailureAccrualPolicy,
    R: FailureAccrualPolicy
{
    #[inline]
    fn record_success(&mut self) {
        self.left.record_success();
        self.right.record_success();
    }

    #[inline]
    fn mark_dead_on_failure(&mut self) -> Option<Duration> {
        let left = self.left.mark_dead_on_failure();
        let right = self.right.mark_dead_on_failure();

        match (left, right) {
            (Some(_), None) => left,
            (None, Some(_)) => right,
            (Some(l), Some(r)) => {
                Some(l.max(r))
            }
            _ => None
        }
    }

    #[inline]
    fn revived(&mut self) {
        self.left.revived();
        self.right.revived();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::backoff;
    use super::super::mock_clock;

    mod consecutive_failures_policy {
        use super::*;

        #[test]
        fn fail_on_nth_attempt() {
            let mut policy = consecutive_failures(3, constant_backoff());

            assert_eq!(None, policy.mark_dead_on_failure());
            assert_eq!(None, policy.mark_dead_on_failure());
            assert_eq!(Some(5.seconds()), policy.mark_dead_on_failure());
        }

        #[test]
        fn reset_to_zero_on_revived() {
            let mut policy = consecutive_failures(3, constant_backoff());

            assert_eq!(None, policy.mark_dead_on_failure());

            policy.revived();

            assert_eq!(None, policy.mark_dead_on_failure());
            assert_eq!(None, policy.mark_dead_on_failure());
            assert_eq!(Some(5.seconds()), policy.mark_dead_on_failure());
        }

        #[test]
        fn reset_to_zero_on_success() {
            let mut policy = consecutive_failures(3, constant_backoff());

            assert_eq!(None, policy.mark_dead_on_failure());

            policy.record_success();

            assert_eq!(None, policy.mark_dead_on_failure());
            assert_eq!(None, policy.mark_dead_on_failure());
            assert_eq!(Some(5.seconds()), policy.mark_dead_on_failure());
        }

        #[test]
        fn iterates_over_backoff() {
            let exp_backoff = exp_backoff();
            let mut policy = consecutive_failures(1, exp_backoff.clone());

            for i in exp_backoff.take(6) {
                assert_eq!(Some(i), policy.mark_dead_on_failure());
            }
        }
    }

    mod success_rate_policy {
        use super::*;

        #[test]
        fn fail_when_success_rate_not_met() {
            mock_clock::freeze(|time| {
                let exp_backoff = exp_backoff();
                let success_rate_duration = 30.seconds();
                let mut policy = success_rate(0.5, success_rate_duration, exp_backoff.clone());

                assert_eq!(None, policy.mark_dead_on_failure());

                // Advance the time with 'success_rate_duration'.
                // All mark_dead_on_failure calls should now return Some(Duration),
                // and should iterate over expBackoffList.
                time.advance(success_rate_duration);

                for i in exp_backoff.take(6) {
                    assert_eq!(Some(i), policy.mark_dead_on_failure());
                }
            })
        }

        #[test]
        fn revived_resets_failures() {
            mock_clock::freeze(|time| {
                let exp_backoff = exp_backoff();
                let success_rate_duration = 30.seconds();
                let mut policy = success_rate(0.5, success_rate_duration, exp_backoff.clone());

                time.advance(success_rate_duration);
                for i in exp_backoff.take(6) {
                    assert_eq!(Some(i), policy.mark_dead_on_failure());
                }

                policy.revived();

                // Make sure the failure status has been reset.
                // This will also be registered as the timestamp of the first request.
                assert_eq!(None, policy.mark_dead_on_failure());

                // One failure after 'success_rate_duration' should mark the node dead again.
                time.advance(success_rate_duration);
                assert!(policy.mark_dead_on_failure().is_some())
            })
        }

        #[test]
        fn fractional_success_rate() {
            mock_clock::freeze(|time| {
                let exp_backoff = exp_backoff();
                let success_rate_duration = 100.seconds();
                let mut policy = success_rate(0.5, success_rate_duration, exp_backoff.clone());

                for _i in 0..100 {
                    time.advance(1.seconds());
                    policy.record_success();
                }

                // With a window of 100 seconds, it will take 100 * ln(2) + 1 = 70 seconds of failures
                // for the success rate to drop below 0.5 (half-life).
                for _i in 0..69 {
                    time.advance(1.seconds());
                    assert_eq!(None, policy.mark_dead_on_failure(), "n={}", _i);
                }

                // 70th failure should make markDeadOnFailure() return Some(_)
                time.advance(1.seconds());
                assert_eq!(Some(5.seconds()), policy.mark_dead_on_failure());
            })
        }
    }

    mod or_else_policy {
        use super::*;

        #[test]
        fn compose_policies() {
            let mut policy = consecutive_failures(3, constant_backoff())
                    .or_else(success_rate(0.5, 10.seconds(), constant_backoff()));

            policy.record_success();
            assert_eq!(None, policy.mark_dead_on_failure());
        }
    }

    fn constant_backoff() -> backoff::Constant {
        backoff::constant(5.seconds())
    }

    fn exp_backoff() -> backoff::Exponential {
        backoff::exponential(5.seconds(), 60.seconds())
    }

    trait IntoDuration {
        fn seconds(self) -> Duration;
    }

    impl IntoDuration for u64 {
        fn seconds(self) -> Duration {
            Duration::from_secs(self)
        }
    }
}