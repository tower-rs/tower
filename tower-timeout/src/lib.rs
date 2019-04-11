//! Tower middleware that applies a timeout to requests.
//!
//! If the response does not complete within the specified timeout, the response
//! will be aborted.

#![doc(html_root_url = "https://docs.rs/tower-timeout/0.1.0")]
#![deny(missing_debug_implementations, missing_docs, rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]
#![cfg_attr(test, deny(warnings))]

pub mod error;
pub mod future;
mod layer;

pub use crate::layer::TimeoutLayer;

use crate::{error::Error, future::ResponseFuture};
use futures::Poll;
use tokio_timer::{clock, Delay};

use tower_service::Service;

use std::time::Duration;

/// Applies a timeout to requests.
#[derive(Debug, Clone)]
pub struct Timeout<T> {
    inner: T,
    timeout: Duration,
}

// ===== impl Timeout =====

impl<T> Timeout<T> {
    /// Creates a new Timeout
    pub fn new(inner: T, timeout: Duration) -> Self {
        Timeout { inner, timeout }
    }
}

impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request>,
    Error: From<S::Error>,
{
    type Response = S::Response;
    type Error = Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response = self.inner.call(request);
        let sleep = Delay::new(clock::now() + self.timeout);

        ResponseFuture::new(response, sleep)
    }
}
