//! Contains `OptionService` and related types and functions.
//!
//! See `OptionService` documentation for more details.
//!

/// Error types for `OptionalService`.
pub mod error;
/// Future types for `OptionalService`.
pub mod future;

use self::{error::Error, future::ResponseFuture};
use std::task::{Context, Poll};
use tower_service::Service;

/// Optionally forwards requests to an inner service.
///
/// If the inner service is `None`, `Error::None` is returned as the response.
#[derive(Debug)]
pub struct Optional<T> {
    inner: Option<T>,
}

impl<T> Optional<T> {
    /// Create a new `OptionService`
    pub fn new<Request>(inner: Option<T>) -> Optional<T>
    where
        T: Service<Request>,
        T::Error: Into<Error>,
    {
        Optional { inner }
    }
}

impl<T, Request> Service<Request> for Optional<T>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    type Response = T::Response;
    type Error = Error;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.inner {
            Some(ref mut inner) => match inner.poll_ready(cx) {
                Poll::Ready(r) => Poll::Ready(r.map_err(Into::into)),
                Poll::Pending => Poll::Pending,
            },
            // None services are always ready
            None => Poll::Ready(Ok(())),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let inner = self.inner.as_mut().map(|i| i.call(request));
        ResponseFuture::new(inner)
    }
}
