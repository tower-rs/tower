use std::time::Duration;

use super::{
    policy::{CircuitPolicy, ConsecutiveFailures},
    service::CircuitBreaker,
};

/// [`Layer`] that wraps services in a [`CircuitBreaker`].
///
/// Construct with [`CircuitBreakerLayer::new`] for the standard
/// [`ConsecutiveFailures`] policy, or with [`CircuitBreakerLayer::with_policy`]
/// to supply any custom [`CircuitPolicy`].
///
/// [`Layer`]: tower_layer::Layer
#[derive(Clone, Debug)]
pub struct CircuitBreakerLayer<P> {
    policy: P,
}

impl CircuitBreakerLayer<ConsecutiveFailures> {
    /// Create a layer using the built-in [`ConsecutiveFailures`] policy.
    ///
    /// - `failure_threshold`: consecutive failures before the circuit opens.
    /// - `success_threshold`: fraction of probes (0.0–1.0) that must succeed
    ///   during [`HalfOpen`][crate::circuit_breaker::CircuitStatus::HalfOpen]
    ///   before the circuit closes again.
    /// - `timeout`: how long to stay open before sending the first probe.
    pub fn new(failure_threshold: usize, success_threshold: f64, timeout: Duration) -> Self {
        Self {
            policy: ConsecutiveFailures::new(failure_threshold, success_threshold, timeout),
        }
    }
}

impl<P: CircuitPolicy + Clone> CircuitBreakerLayer<P> {
    /// Create a layer using a custom [`CircuitPolicy`].
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use tower::circuit_breaker::{CircuitBreakerLayer, ConsecutiveFailures};
    /// use std::time::Duration;
    ///
    /// // Using the built-in policy explicitly:
    /// let policy = ConsecutiveFailures::new(5, 0.8, Duration::from_secs(30));
    /// let layer  = CircuitBreakerLayer::with_policy(policy);
    ///
    /// // Or bring your own:
    /// let layer = CircuitBreakerLayer::with_policy(MyLatencyPolicy::new());
    /// ```
    pub fn with_policy(policy: P) -> Self {
        Self { policy }
    }
}

impl<S, P: CircuitPolicy + Clone> tower_layer::Layer<S> for CircuitBreakerLayer<P> {
    type Service = CircuitBreaker<S, P>;

    fn layer(&self, inner: S) -> Self::Service {
        CircuitBreaker::new(inner, self.policy.clone())
    }
}
