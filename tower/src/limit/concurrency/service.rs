use super::future::ResponseFuture;
use crate::semaphore::Semaphore;
use tower_service::Service;

use futures_core::ready;
use std::task::{Context, Poll};

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle.
#[derive(Debug, Clone)]
pub struct ConcurrencyLimit<T> {
    inner: T,
    semaphore: Semaphore,
}

impl<T> ConcurrencyLimit<T> {
    /// Create a new concurrency limiter.
    pub fn new(inner: T, max: usize) -> Self {
        ConcurrencyLimit {
            inner,
            semaphore: Semaphore::new(max),
        }
    }

    /// Get a reference to the inner service
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Get a mutable reference to the inner service
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Consume `self`, returning the inner service
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<S, Request> Service<Request> for ConcurrencyLimit<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // First, poll the semaphore...
        ready!(self.semaphore.poll_ready(cx));
        // ...and if it's ready, poll the inner service.
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // Take the permit
        let permit = self
            .semaphore
            .take_permit()
            .expect("max requests in-flight; poll_ready must be called first");

        // Call the inner service
        let future = self.inner.call(request);

        ResponseFuture::new(future, permit)
    }
}

#[cfg(feature = "load")]
impl<S> crate::load::Load for ConcurrencyLimit<S>
where
    S: crate::load::Load,
{
    type Metric = S::Metric;
    fn load(&self) -> Self::Metric {
        self.inner.load()
    }
}
