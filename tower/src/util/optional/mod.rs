//! Contains [`Optional`] and related types and functions.
//!
//! See [`Optional`] documentation for more details.

/// Error types for [`Optional`].
pub mod error;
/// Future types for [`Optional`].
pub mod future;

use self::future::ResponseFuture;
use std::task::{Context, Poll};
use tower_service::{Call, Service};

/// Optionally forwards requests to an inner service.
///
/// If the inner service is [`None`], [`optional::None`] is returned as the response.
///
/// [`optional::None`]: crate::util::error::optional::None
#[derive(Debug)]
pub struct Optional<T> {
    inner: Option<T>,
}

impl<T> Optional<T> {
    /// Create a new [`Optional`].
    pub fn new<Request>(inner: Option<T>) -> Optional<T>
    where
        T: Service<Request>,
        T::Error: Into<crate::BoxError>,
    {
        Optional { inner }
    }
}

impl<T, Request> Service<Request> for Optional<T>
where
    T: Service<Request>,
    T::Error: Into<crate::BoxError>,
{
    type Call = Optional<T::Call>;
    type Response = T::Response;
    type Error = crate::BoxError;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Call, Self::Error>> {
        match self.inner {
            Some(ref mut inner) => match inner.poll_ready(cx) {
                Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                Poll::Ready(Ok(inner)) => Poll::Ready(Ok(Optional { inner: Some(inner) })),
                Poll::Pending => Poll::Pending,
            },
            // None services are always ready
            None => Poll::Ready(Ok(Optional { inner: None })),
        }
    }
}
impl<T, Request> Call<Request> for Optional<T>
where
    T: Call<Request>,
    T::Error: Into<crate::BoxError>,
{
    type Response = T::Response;
    type Error = crate::BoxError;
    type Future = ResponseFuture<T::Future>;
    fn call(mut self, request: Request) -> Self::Future {
        let inner = self.inner.take().map(|i| i.call(request));
        ResponseFuture::new(inner)
    }
}
