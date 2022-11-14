//! Middleware that applies a timeout to requests.
//!
//! If the response does not complete within the specified timeout, the response
//! will be aborted.

pub mod error;
pub mod future;
mod layer;

use crate::timeout::error::Elapsed;

pub use self::layer::GlobalTimeoutLayer;
pub use self::layer::TimeoutLayer;

use self::future::ResponseFuture;
use futures_core::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::Sleep;
use tower_service::Service;

/// Applies a timeout to requests.
#[derive(Debug, Clone)]
pub struct Timeout<T> {
    inner: T,
    timeout: Duration,
}

// ===== impl Timeout =====

impl<T> Timeout<T> {
    /// Creates a new [`Timeout`]
    pub fn new(inner: T, timeout: Duration) -> Self {
        Timeout { inner, timeout }
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

impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request>,
    S::Error: Into<crate::BoxError>,
{
    type Response = S::Response;
    type Error = crate::BoxError;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.inner.poll_ready(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(r) => Poll::Ready(r.map_err(Into::into)),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response = self.inner.call(request);
        let sleep = tokio::time::sleep(self.timeout);

        ResponseFuture::new(response, Box::pin(sleep))
    }
}

/// Applies a timeout to requests (including poll_ready).
/// The main difference with [`Timeout`] is that we're starting the timeout before the `poll_ready` call of the inner service.
/// Which means you can use it with the `RateLimit` service if you want to abort a rate limited call
#[derive(Debug)]
pub struct GlobalTimeout<T> {
    inner: T,
    timeout: Duration,
    sleep: Option<Pin<Box<Sleep>>>,
}

// ===== impl GlobalTimeout =====

impl<T> GlobalTimeout<T> {
    /// Creates a new [`GlobalTimeout`]
    pub fn new(inner: T, timeout: Duration) -> Self {
        GlobalTimeout {
            inner,
            timeout,
            sleep: None,
        }
    }
}

impl<S, Request> Service<Request> for GlobalTimeout<S>
where
    S: Service<Request>,
    S::Error: Into<crate::BoxError>,
{
    type Response = S::Response;
    type Error = crate::BoxError;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.sleep.is_none() {
            self.sleep = Some(Box::pin(tokio::time::sleep(self.timeout)));
        }
        match self.inner.poll_ready(cx) {
            Poll::Pending => {}
            Poll::Ready(r) => return Poll::Ready(r.map_err(Into::into)),
        };

        // Checking if we don't timeout on `poll_ready`
        if Pin::new(
            &mut self
                .sleep
                .as_mut()
                .expect("we can unwrap because we set it just before"),
        )
        .poll(cx)
        .is_ready()
        {
            tracing::trace!("timeout exceeded.");
            self.sleep = None;

            return Poll::Ready(Err(Elapsed::new().into()));
        }

        Poll::Pending
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response = self.inner.call(request);

        ResponseFuture::new(
            response,
            self.sleep
                .take()
                .expect("poll_ready must been called before"),
        )
    }
}
