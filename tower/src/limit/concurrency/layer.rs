use std::sync::Arc;

use super::ConcurrencyLimit;
use tokio::sync::Semaphore;
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

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle.
/// This variant accepts a owned semaphore (`Arc<Semaphore>`) which can
/// be reused across services.
#[derive(Debug, Clone)]
pub struct GlobalConcurrencyLimitLayer {
    semaphore: Arc<Semaphore>,
}

impl GlobalConcurrencyLimitLayer {
    /// Create a new `GlobalConcurrencyLimitLayer`.
    pub fn new(semaphore: Arc<Semaphore>) -> Self {
        GlobalConcurrencyLimitLayer { semaphore }
    }
}

impl<S> Layer<S> for GlobalConcurrencyLimitLayer {
    type Service = ConcurrencyLimit<S>;

    fn layer(&self, service: S) -> Self::Service {
        ConcurrencyLimit::new_shared(service, self.semaphore.clone())
    }
}
