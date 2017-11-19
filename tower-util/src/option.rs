//! Contains `OptionService` and related types and functions.
//!
//! See `OptionService` documentation for more details.
//!
use futures::{Future, Poll};
use tower::Service;

/// Optionally forwards requests to an inner service.
///
/// If the inner service is `None`, `Error::None` is returned as the response.
pub struct OptionService<T> {
    inner: Option<T>,
}

/// Response future returned by `OptionService`.
pub struct ResponseFuture<T> {
    inner: Option<T>,
}

/// Error produced by `OptionService` responding to a request.
#[derive(Debug)]
pub enum Error<T> {
    Inner(T),
    None,
}

// ===== impl OptionService =====

impl<T> OptionService<T> {
    /// Returns an `OptionService` that forwards requests to `inner`.
    pub fn some(inner: T) -> Self {
        OptionService { inner: Some(inner) }
    }

    /// Returns an `OptionService` that responds to all requests with
    /// `Error::None`.
    pub fn none() -> Self {
        OptionService { inner: None }
    }
}

impl<T> Service for OptionService<T>
where T: Service,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = Error<T::Error>;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        match self.inner {
            Some(ref mut inner) => inner.poll_ready().map_err(Error::Inner),
            // None services are always ready
            None => Ok(().into()),
        }
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        let inner = self.inner.as_mut().map(|i| i.call(request));
        ResponseFuture { inner }
    }
}

// ===== impl ResponseFuture =====

impl<T> Future for ResponseFuture<T>
where T: Future,
{
    type Item = T::Item;
    type Error = Error<T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.inner {
            Some(ref mut inner) => inner.poll().map_err(Error::Inner),
            None => Err(Error::None),
        }
    }
}
