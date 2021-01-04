//! Conditionally dispatch requests to the inner service based on the result of
//! a predicate.

pub mod error;
pub mod future;
mod layer;
mod predicate;

pub use self::{layer::FilterLayer, predicate::Predicate};

use self::{error::Error, future::ResponseFuture};
use futures_core::ready;
use std::task::{Context, Poll};
use tower_service::Service;

/// Conditionally dispatch requests to the inner service based on a predicate.
#[derive(Clone, Debug)]
pub struct Filter<T, U> {
    inner: T,
    predicate: U,
}

impl<T, U> Filter<T, U> {
    #[allow(missing_docs)]
    pub fn new(inner: T, predicate: U) -> Self {
        Filter { inner, predicate }
    }
}

impl<T, U, Request> Service<Request> for Filter<T, U>
where
    U: Predicate<Request>,
    T: Service<U::Request> + Clone,
    crate::BoxError: From<T::Error>,
{
    type Response = T::Response;
    type Error = crate::BoxError;
    type Future = ResponseFuture<U::Future, T, Request>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(ready!(self.inner.poll_ready(cx)).map_err(error::Error::inner))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        use std::mem;

        let inner = self.inner.clone();
        let inner = mem::replace(&mut self.inner, inner);

        // Check the request
        let check = self.predicate.check(&request);

        ResponseFuture::new(request, check, inner)
    }
}
