//! Tower middleware that applies a timeout to requests.
//!
//! If the response does not complete within the specified timeout, the response
//! will be aborted.

#![doc(html_root_url = "https://docs.rs/tower-timeout/0.1.0")]
#![deny(missing_debug_implementations, missing_docs)]
#![cfg_attr(test, deny(warnings))]

extern crate futures;
extern crate tokio_timer;
extern crate tower_service;

use futures::{Async, Future, Poll};
use tokio_timer::{clock, Delay};
use tower_service::Service;

use std::time::Duration;
use std::{error::Error as StdError, fmt};

/// Applies a timeout to requests.
#[derive(Debug)]
pub struct Timeout<T> {
    inner: T,
    timeout: Duration,
}

type Error = Box<StdError + Send + Sync>;

#[derive(Debug)]
struct ElapsedTimer;

impl StdError for ElapsedTimer {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self)
    }
}

impl fmt::Display for ElapsedTimer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Timer elapsed")
    }
}

/// `Timeout` response future
#[derive(Debug)]
pub struct ResponseFuture<T> {
    response: T,
    sleep: Delay,
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
        self.inner.poll_ready().map_err(|e| e.into())
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
        match self.response.poll() {
            Ok(Async::Ready(v)) => return Ok(Async::Ready(v)),
            Ok(Async::NotReady) => {}
            Err(e) => return Err(e.into()),
        }

        // Now check the sleep
        match self.sleep.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => Err(ElapsedTimer.into()),
            Err(e) => Err(e.into()),
        }
    }
}
