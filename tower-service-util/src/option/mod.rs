//! Contains `OptionService` and related types and functions.
//!
//! See `OptionService` documentation for more details.
//!

pub mod error;
pub mod future;

use self::error::Error;
use self::future::ResponseFuture;
use futures::Poll;
use tower_service::Service;

/// Optionally forwards requests to an inner service.
///
/// If the inner service is `None`, `Error::None` is returned as the response.
pub struct OptionService<T> {
    inner: Option<T>,
}

// ===== impl OptionService =====

impl<T> OptionService<T> {
    /// Create a new `OptionService`
    pub fn new<Request>(inner: Option<T>) -> OptionService<T>
    where
        T: Service<Request>,
        T::Error: Into<Error>,
    {
        OptionService { inner }
    }
}

impl<T, Request> Service<Request> for OptionService<T>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    type Response = T::Response;
    type Error = Error;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        match self.inner {
            Some(ref mut inner) => inner.poll_ready().map_err(Into::into),
            // None services are always ready
            None => Ok(().into()),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let inner = self.inner.as_mut().map(|i| i.call(request));
        ResponseFuture::new(inner)
    }
}
