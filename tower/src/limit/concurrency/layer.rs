use super::ConcurrencyLimit;
use tower_layer::Layer;

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle.
#[derive(Debug, Clone)]
pub struct ConcurrencyLimitLayer {
    max: usize,
}

impl ConcurrencyLimitLayer {
    /// Create a new concurrency limit layer.
    pub fn new(max: usize) -> Self {
        ConcurrencyLimitLayer { max }
    }
}

impl<S> Layer<S> for ConcurrencyLimitLayer {
    type Service = ConcurrencyLimit<S>;

    fn layer(&self, service: S) -> Self::Service {
        ConcurrencyLimit::new(service, self.max)
    }
}
