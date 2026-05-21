//! Middleware that applies a timeout to requests.
//!
//! If the response does not complete within the specified timeout, the response
//! will be aborted.

pub mod error;
pub mod future;
mod layer;

pub use self::layer::TimeoutLayer;

use self::future::ResponseFuture;
use std::task::{Context, Poll};
use std::time::Duration;
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
    pub const fn new(inner: T, timeout: Duration) -> Self {
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

        ResponseFuture::new(response, sleep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        convert::Infallible,
        future::Future,
        pin::Pin,
        task::{Context, Poll},
        time::Duration,
    };
    use tokio::time::sleep;
    use tower_service::Service;

    struct SlowService(Duration);

    impl Service<()> for SlowService {
        type Response = ();
        type Error = Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<(), Infallible>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: ()) -> Self::Future {
            let delay = self.0;
            Box::pin(async move {
                sleep(delay).await;
                Ok(())
            })
        }
    }

    struct FastService;

    impl Service<()> for FastService {
        type Response = &'static str;
        type Error = Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<&'static str, Infallible>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: ()) -> Self::Future {
            Box::pin(async move { Ok("ok") })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn elapsed_error_when_timeout_exceeded() {
        let mut svc = Timeout::new(SlowService(Duration::from_secs(10)), Duration::from_secs(1));

        let res = svc.call(()).await;
        assert!(res.is_err());

        let err = res.unwrap_err();
        assert!(err.downcast_ref::<error::Elapsed>().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn response_passes_through_when_under_timeout() {
        let mut svc = Timeout::new(FastService, Duration::from_secs(1));

        let res = svc.call(()).await;
        assert_eq!(res.unwrap(), "ok");
    }
}
