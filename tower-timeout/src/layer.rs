use crate::Timeout;
use std::time::Duration;
use tower_layer::Layer;

/// Applies a timeout to requests via the supplied inner service.
#[derive(Debug)]
pub struct TimeoutLayer {
    timeout: Duration,
}

impl TimeoutLayer {
    /// Create a timeout from a duration
    pub fn new(timeout: Duration) -> Self {
        TimeoutLayer { timeout }
    }
}

impl<'a, S: 'a> Layer<'a, S> for TimeoutLayer {
    type Service = Timeout<'a, S>;

    fn layer(&self, service: &'a mut S) -> Self::Service {
        Timeout::new(service, self.timeout)
    }
}
