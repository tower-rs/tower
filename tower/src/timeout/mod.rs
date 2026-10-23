//! Middleware that applies a timeout to requests.
//!
//! If the response does not complete within the specified timeout, the response
//! will be aborted.

pub mod error;
pub mod future;
mod layer;

pub use self::layer::TimeoutLayer;

use self::future::ResponseFuture;
use crate::timer::{Timer, TokioTimer};
use std::task::{Context, Poll};
use std::time::Duration;
use tower_service::Service;

/// Applies a timeout to requests.
///
/// Uses [`TokioTimer`] unless created with [`Timeout::with_timer`].
#[derive(Debug, Clone)]
pub struct Timeout<T, Tm = TokioTimer> {
    inner: T,
    timeout: Duration,
    timer: Tm,
}

// ===== impl Timeout =====

impl<T> Timeout<T> {
    /// Creates a new [`Timeout`]
    pub const fn new(inner: T, timeout: Duration) -> Self {
        Timeout::with_timer(inner, timeout, TokioTimer)
    }
}

impl<T, Tm> Timeout<T, Tm> {
    /// Creates a new [`Timeout`] that sleeps using `timer`
    pub const fn with_timer(inner: T, timeout: Duration, timer: Tm) -> Self {
        Timeout {
            inner,
            timeout,
            timer,
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

impl<S, Tm, Request> Service<Request> for Timeout<S, Tm>
where
    S: Service<Request>,
    S::Error: Into<crate::BoxError>,
    Tm: Timer,
{
    type Response = S::Response;
    type Error = crate::BoxError;
    type Future = ResponseFuture<S::Future, Tm::Sleep>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.inner.poll_ready(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(r) => Poll::Ready(r.map_err(Into::into)),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response = self.inner.call(request);
        let sleep = self.timer.sleep(self.timeout);

        ResponseFuture::new(response, sleep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::{pending, ready, Pending, Ready};
    use std::time::Instant;
    use tokio_test::{assert_ready_err, task};

    #[derive(Clone, Copy)]
    struct ExpiredTimer;

    impl Timer for ExpiredTimer {
        type Sleep = Ready<()>;

        fn sleep(&self, _: Duration) -> Self::Sleep {
            ready(())
        }

        fn sleep_until(&self, _: Instant) -> Self::Sleep {
            ready(())
        }
    }

    struct Never;

    impl Service<()> for Never {
        type Response = ();
        type Error = crate::BoxError;
        type Future = Pending<Result<(), crate::BoxError>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: ()) -> Self::Future {
            pending()
        }
    }

    #[test]
    fn custom_timer_without_tokio_runtime() {
        let mut service = Timeout::with_timer(Never, Duration::from_secs(1), ExpiredTimer);
        let mut response = task::spawn(service.call(()));

        let err = assert_ready_err!(response.poll());
        assert!(err.is::<error::Elapsed>());
    }
}
