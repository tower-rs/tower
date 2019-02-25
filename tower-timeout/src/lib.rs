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
use tokio_timer::{clock, Delay, Error as TimerError};
use tower_layer::Layer;
use tower_service::Service;

use std::time::Duration;
use std::{error, fmt};

/// Applies a timeout to requests sent via the inner service.
#[derive(Debug)]
pub struct Timeout<T> {
    inner: T,
    timeout: Duration,
}

/// Applies a timeout to requests.
#[derive(Debug)]
pub struct TimeoutLayer {
    timeout: Duration,
}

/// Errors produced by `Timeout`.
#[derive(Debug)]
pub struct Error<T>(Kind<T>);

/// Timeout error variants
#[derive(Debug)]
enum Kind<T> {
    /// Inner value returned an error
    Inner(T),

    /// The timeout elapsed.
    Elapsed,

    /// Timer returned an error.
    Timer(TimerError),
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
{
    type Response = S::Response;
    type Error = Error<S::Error>;
    type Service = Timeout<S>;

    fn layer(&self, service: S) -> Self::Service {
        Timeout::new(service, self.timeout)
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
{
    type Response = S::Response;
    type Error = Error<S::Error>;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(|e| Error(Kind::Inner(e)))
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
{
    type Item = T::Item;
    type Error = Error<T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // First, try polling the future
        match self.response.poll() {
            Ok(Async::Ready(v)) => return Ok(Async::Ready(v)),
            Ok(Async::NotReady) => {}
            Err(e) => return Err(Error(Kind::Inner(e))),
        }

        // Now check the sleep
        match self.sleep.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(_)) => Err(Error(Kind::Elapsed)),
            Err(e) => Err(Error(Kind::Timer(e))),
        }
    }
}

// ===== impl Error =====

impl<T> fmt::Display for Error<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Kind::Inner(ref why) => fmt::Display::fmt(why, f),
            Kind::Elapsed => f.pad("request timed out"),
            Kind::Timer(ref why) => fmt::Display::fmt(why, f),
        }
    }
}

impl<T> error::Error for Error<T>
where
    T: error::Error + 'static,
{
    fn description(&self) -> &str {
        match self.0 {
            Kind::Inner(ref e) => e.description(),
            Kind::Elapsed => "request timed out",
            Kind::Timer(ref e) => e.description(),
        }
    }

    fn source(&self) -> Option<&(error::Error + 'static)> {
        let kind = &self.0;
        if let Kind::Inner(ref why) = kind {
            Some(why)
        } else {
            None
        }
    }
}

// ===== impl Error =====

impl<T> Error<T> {
    /// Create a new `Error` representing the inner value completing with `Err`.
    pub fn inner(err: T) -> Error<T> {
        Error(Kind::Inner(err))
    }

    /// Returns `true` if the error was caused by the inner value completing
    /// with `Err`.
    pub fn is_inner(&self) -> bool {
        match self.0 {
            Kind::Inner(_) => true,
            _ => false,
        }
    }

    /// Consumes `self`, returning the inner future error.
    pub fn into_inner(self) -> Option<T> {
        match self.0 {
            Kind::Inner(err) => Some(err),
            _ => None,
        }
    }

    /// Create a new `Error` representing the inner value not completing before
    /// the deadline is reached.
    pub fn elapsed() -> Error<T> {
        Error(Kind::Elapsed)
    }

    /// Returns `true` if the error was caused by the inner value not completing
    /// before the deadline is reached.
    pub fn is_elapsed(&self) -> bool {
        match self.0 {
            Kind::Elapsed => true,
            _ => false,
        }
    }

    /// Creates a new `Error` representing an error encountered by the timer
    /// implementation
    pub fn timer(err: TimerError) -> Error<T> {
        Error(Kind::Timer(err))
    }

    /// Returns `true` if the error was caused by the timer.
    pub fn is_timer(&self) -> bool {
        match self.0 {
            Kind::Timer(_) => true,
            _ => false,
        }
    }

    /// Consumes `self`, returning the error raised by the timer implementation.
    pub fn into_timer(self) -> Option<TimerError> {
        match self.0 {
            Kind::Timer(err) => Some(err),
            _ => None,
        }
    }
}
