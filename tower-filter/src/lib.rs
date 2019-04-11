#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]

//! Conditionally dispatch requests to the inner service based on the result of
//! a predicate.

pub mod error;
pub mod future;
mod layer;
mod predicate;

pub use crate::{layer::FilterLayer, predicate::Predicate};

use crate::{error::Error, future::ResponseFuture};
use futures::Poll;
use tower_service::Service;

#[derive(Debug)]
pub struct Filter<T, U> {
    inner: T,
    predicate: U,
}

impl<T, U> Filter<T, U> {
    pub fn new(inner: T, predicate: U) -> Self {
        Filter { inner, predicate }
    }
}

impl<T, U, Request> Service<Request> for Filter<T, U>
where
    T: Service<Request> + Clone,
    T::Error: Into<error::Source>,
    U: Predicate<Request>,
{
    type Response = T::Response;
    type Error = Error;
    type Future = ResponseFuture<U::Future, T, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(error::Error::inner)
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
