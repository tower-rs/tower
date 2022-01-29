use super::future::ResponseFuture;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::PollSemaphore;
use tower_service::Service;

use futures_core::ready;
use std::{
    sync::Arc,
    task::{Context, Poll},
};

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle.
#[derive(Debug)]
pub struct ConcurrencyLimit<T> {
    inner: T,
    semaphore: PollSemaphore,
}

impl<T> ConcurrencyLimit<T> {
    /// Create a new concurrency limiter.
    pub fn new(inner: T, max: usize) -> Self {
        Self::with_semaphore(inner, Arc::new(Semaphore::new(max)))
    }

    /// Create a new concurrency limiter with a provided shared semaphore
    pub fn with_semaphore(inner: T, semaphore: Arc<Semaphore>) -> Self {
        ConcurrencyLimit {
            inner,
            semaphore: PollSemaphore::new(semaphore),
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

#[derive(Debug)]
pub struct Token<T> {
    inner: T,
    permit: OwnedSemaphorePermit,
}

impl<S, Request> Service<Request> for ConcurrencyLimit<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Token = Token<S::Token>;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Token, Self::Error>> {
        let permit = ready!(self.semaphore.poll_acquire(cx)).expect(
            "ConcurrencyLimit semaphore is never closed, so `poll_acquire` \
                should never fail",
        );

        // Once we've acquired a permit, poll the inner service.
        self.inner
            .poll_ready(cx)
            .map_ok(move |inner| Token { inner, permit })
    }

    fn call(&mut self, token: Self::Token, request: Request) -> Self::Future {
        // Call the inner service
        let future = self.inner.call(token.inner, request);

        ResponseFuture::new(future, token.permit)
    }
}

impl<T: Clone> Clone for ConcurrencyLimit<T> {
    fn clone(&self) -> Self {
        // Since we hold an `OwnedSemaphorePermit`, we can't derive `Clone`.
        // Instead, when cloning the service, create a new service with the
        // same semaphore, but with the permit in the un-acquired state.
        Self {
            inner: self.inner.clone(),
            semaphore: self.semaphore.clone(),
        }
    }
}

#[cfg(feature = "load")]
#[cfg_attr(docsrs, doc(cfg(feature = "load")))]
impl<S> crate::load::Load for ConcurrencyLimit<S>
where
    S: crate::load::Load,
{
    type Metric = S::Metric;
    fn load(&self) -> Self::Metric {
        self.inner.load()
    }
}
