use super::{GlobalTimeout, Timeout};
use std::time::Duration;
use tower_layer::Layer;

/// Applies a timeout to requests via the supplied inner service.
#[derive(Debug, Clone)]
pub struct TimeoutLayer {
    timeout: Duration,
}

impl TimeoutLayer {
    /// Create a timeout from a duration
    pub fn new(timeout: Duration) -> Self {
        TimeoutLayer { timeout }
    }
}

impl<S> Layer<S> for TimeoutLayer {
    type Service = Timeout<S>;

    fn layer(&self, service: S) -> Self::Service {
        Timeout::new(service, self.timeout)
    }
}

/// Applies a timeout to requests via the supplied inner service (including poll_ready).
/// The main difference with [`TimeoutLayer`] is that we're starting the timeout before the `poll_ready` call of the inner service.
/// Which means you can use it with the `RateLimitLayer` service if you want to abort a rate limited call
#[derive(Debug, Clone)]
pub struct GlobalTimeoutLayer {
    timeout: Duration,
}

impl GlobalTimeoutLayer {
    /// Create a timeout from a duration
    pub fn new(timeout: Duration) -> Self {
        GlobalTimeoutLayer { timeout }
    }
}

impl<S> Layer<S> for GlobalTimeoutLayer {
    type Service = GlobalTimeout<S>;

    fn layer(&self, service: S) -> Self::Service {
        GlobalTimeout::new(service, self.timeout)
    }
}
