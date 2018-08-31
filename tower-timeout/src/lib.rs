//! Tower middleware that applies a timeout to requests.
//!
//! If the response does not complete within the specified timeout, the response
//! will be aborted.

extern crate futures;
extern crate tower_service;
extern crate tokio_timer;

use futures::{Future, Poll, Async};
use tower_service::Service;
use tokio_timer::{clock, Delay};

use std::{error, fmt};
use std::time::Duration;

/// Applies a timeout to requests.
#[derive(Debug)]
pub struct Timeout<T> {
    inner: T,
    timeout: Duration,
}

/// Errors produced by `Timeout`.
#[derive(Debug)]
pub enum Error<T> {
    /// The inner service produced an error
    Inner(T),

    /// The request did not complete within the specified timeout.
    Timeout,
}

/// `Timeout` response future
#[derive(Debug)]
pub struct ResponseFuture<T> {
    response: T,
    sleep: Delay,
}

// ===== impl Timeout =====

impl<T> Timeout<T> {
    pub fn new(inner: T, timeout: Duration) -> Self {
        Timeout {
            inner,
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
            .map_err(Error::Inner)
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        ResponseFuture {
            response: self.inner.call(request),
            sleep: Delay::new(clock::now() + self.timeout),
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
            Err(e) => return Err(Error::Inner(e)),
        }

        // Now check the sleep
        match self.sleep.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => Err(Error::Timeout),
            Err(_) => Err(Error::Timeout),
        }
    }
}


// ===== impl Error =====

impl<T> fmt::Display for Error<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Inner(ref why) => fmt::Display::fmt(why, f),
            Error::Timeout => f.pad("request timed out"),
        }
    }
}

impl<T> error::Error for Error<T>
where
    T: error::Error,
{
    fn cause(&self) -> Option<&error::Error> {
        if let Error::Inner(ref why) = *self {
            Some(why)
        } else {
            None
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Inner(_) => "inner service error",
            Error::Timeout => "request timed out",
        }
    }

}
