//! Tower middleware that applies a timeout to requests.
//!
//! If the response does not complete within the specified timeout, the response
//! will be aborted.

extern crate futures;
extern crate tower_service;
extern crate tokio_timer;

use futures::{Future, Poll, Async};
use tower_service::Service;
use tokio_timer::{Timer, Sleep};

use std::{error, fmt};
use std::time::Duration;

/// Applies a timeout to requests.
#[derive(Debug)]
pub struct Timeout<T> {
    inner: T,
    timer: Timer,
    timeout: Duration,
}

#[macro_use]
mod macros {
    include! { concat!(env!("CARGO_MANIFEST_DIR"), "/../gen_errors.rs") }
}

kind_error!{
    /// Errors produced by `Timeout`.
    #[derive(Debug)]
    pub struct Error from enum ErrorKind {
        /// The inner service produced an error
        Inner(T) => is: is_inner, into: into_inner, borrow: borrow_inner,
        /// The request did not complete within the specified timeout.
        Timeout => fmt: "request timed out", is: is_timeout, into: UNUSED, borrow: UNUSED
    }
}

/// `Timeout` response future
#[derive(Debug)]
pub struct ResponseFuture<T> {
    response: T,
    sleep: Sleep,
}

// ===== impl Timeout =====

impl<T> Timeout<T> {
    pub fn new(inner: T, timer: Timer, timeout: Duration) -> Self {
        Timeout {
            inner,
            timer,
            timeout,
        }
    }
}

impl<S> Service for Timeout<S>
where S: Service,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = Error<S::Error>;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
            .map_err(|e| ErrorKind::Inner(e).into())
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        ResponseFuture {
            response: self.inner.call(request),
            sleep: self.timer.sleep(self.timeout),
        }
    }
}

// ===== impl ResponseFuture =====

impl<T> Future for ResponseFuture<T>
where T: Future,
{
    type Item = T::Item;
    type Error = Error<T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // First, try polling the future
        match self.response.poll() {
            Ok(Async::Ready(v)) => return Ok(Async::Ready(v)),
            Ok(Async::NotReady) => {}
            Err(e) => Err(ErrorKind::Inner(e))?,
        }

        // Now check the sleep
        match self.sleep.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => Err(ErrorKind::Timeout.into()),
            Err(_) => Err(ErrorKind::Timeout.into()),
        }
    }
}
