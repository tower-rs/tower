use std::time::Duration;

use super::service::CircuitBreaker;

/// [`Layer`] that wraps services in a [`CircuitBreaker`].
///
/// [`Layer`]: tower_layer::Layer
#[derive(Clone, Debug)]
pub struct CircuitBreakerLayer {
    failure_threshold: usize,
    success_threshold: f64,
    timeout: Duration,
}

impl CircuitBreakerLayer {
    /// Create a new [`CircuitBreakerLayer`].
    ///
    /// - `failure_threshold`: consecutive failures before the circuit opens.
    /// - `success_threshold`: fraction of probes that must succeed (0.0–1.0)
    ///   before the circuit closes again.
    /// - `timeout`: how long to stay open before attempting recovery.
    pub fn new(failure_threshold: usize, success_threshold: f64, timeout: Duration) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            timeout,
        }
    }
}

impl<S> tower_layer::Layer<S> for CircuitBreakerLayer {
    type Service = CircuitBreaker<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CircuitBreaker::new(
            inner,
            self.failure_threshold,
            self.success_threshold,
            self.timeout,
        )
    }
}
