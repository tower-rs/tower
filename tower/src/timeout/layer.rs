use super::Timeout;
use crate::timer::TokioTimer;
use std::time::Duration;
use tower_layer::Layer;

/// Applies a timeout to requests via the supplied inner service.
#[derive(Debug, Clone)]
pub struct TimeoutLayer<Tm = TokioTimer> {
    timeout: Duration,
    timer: Tm,
}

impl TimeoutLayer {
    /// Create a timeout from a duration
    pub const fn new(timeout: Duration) -> Self {
        TimeoutLayer::with_timer(timeout, TokioTimer)
    }
}

impl<Tm> TimeoutLayer<Tm> {
    /// Create a timeout from a duration that sleeps using `timer`
    pub const fn with_timer(timeout: Duration, timer: Tm) -> Self {
        TimeoutLayer { timeout, timer }
    }
}

impl<S, Tm: Clone> Layer<S> for TimeoutLayer<Tm> {
    type Service = Timeout<S, Tm>;

    fn layer(&self, service: S) -> Self::Service {
        Timeout::with_timer(service, self.timeout, self.timer.clone())
    }
}
