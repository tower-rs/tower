//! Tower middleware that applies a timeout to requests.
//!
//! If the response does not complete within the specified timeout, the response
//! will be aborted.

#![doc(html_root_url = "https://docs.rs/tower-timeout/0.1.0")]
#![deny(missing_debug_implementations, missing_docs)]
#![cfg_attr(test, deny(warnings))]

extern crate futures;
extern crate tokio_timer;
extern crate tower_layer;
extern crate tower_service;

use futures::{Async, Future, Poll};
use tokio_timer::{clock, Delay};
use tower_layer::Layer;
use tower_service::Service;

use std::time::Duration;

use self::error::Elapsed;

type Error = Box<::std::error::Error + Send + Sync>;

/// Applies a timeout to requests.
#[derive(Debug, Clone)]
pub struct Timeout<T> {
    inner: T,
    timeout: Duration,
}

/// Applies a timeout to requests via the supplied inner service.
#[derive(Debug)]
pub struct TimeoutLayer {
    timeout: Duration,
}

/// `Timeout` response future
#[derive(Debug)]
pub struct ResponseFuture<T> {
    response: T,
    sleep: Delay,
}

// ===== impl TimeoutLayer =====

impl TimeoutLayer {
    /// Create a timeout from a duration
    pub fn new(timeout: Duration) -> Self {
        TimeoutLayer { timeout }
    }
}

impl<S, Request> Layer<S, Request> for TimeoutLayer
where
    S: Service<Request>,
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = ();
    type Service = Timeout<S>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(Timeout::new(service, self.timeout))
    }
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
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        ResponseFuture {
            response: self.inner.call(request),
            sleep: Delay::new(clock::now() + self.timeout),
        }
    }
}

// ===== impl ResponseFuture =====

impl<T> Future for ResponseFuture<T>
where
    T: Future,
    T::Error: Into<Error>,
{
    type Item = T::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // First, try polling the future
        match self.response.poll().map_err(Into::into)? {
            Async::Ready(v) => return Ok(Async::Ready(v)),
            Async::NotReady => {}
        }

        // Now check the sleep
        match self.sleep.poll()? {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(_) => Err(Elapsed(()).into()),
        }
    }
}

// ===== impl Error =====

/// Timeout error types
pub mod error {
    use std::{error::Error, fmt};

    /// The timeout elapsed.
    #[derive(Debug)]
    pub struct Elapsed(pub(super) ());

    impl fmt::Display for Elapsed {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.pad("request timed out")
        }
    }

    impl Error for Elapsed {}
}
