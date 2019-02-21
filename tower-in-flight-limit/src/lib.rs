//! Tower middleware that limits the maximum number of in-flight requests for a
//! service.

extern crate futures;
extern crate tower_service;

use tower_service::Service;

use futures::task::AtomicTask;
use futures::{Async, Future, Poll};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::{error, fmt};

#[derive(Debug, Clone)]
pub struct InFlightLimit<T> {
    inner: T,
    state: State,
}

/// Error returned when the service has reached its limit.
#[derive(Debug)]
pub enum Error<T> {
    Upstream(T),
}

#[derive(Debug)]
pub struct ResponseFuture<T> {
    inner: T,
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct State {
    shared: Arc<Shared>,
    reserved: bool,
}

#[derive(Debug)]
struct Shared {
    max: usize,
    curr: AtomicUsize,
    task: AtomicTask,
}

// ===== impl InFlightLimit =====

impl<T> InFlightLimit<T> {
    /// Create a new rate limiter
    pub fn new<Request>(inner: T, max: usize) -> Self
    where
        T: Service<Request>,
    {
        InFlightLimit {
            inner,
            state: State {
                shared: Arc::new(Shared {
                    max,
                    curr: AtomicUsize::new(0),
                    task: AtomicTask::new(),
                }),
                reserved: false,
            },
        }
    }

    /// Get a reference to the inner service
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Get a mutable reference to the inner service
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Consume `self`, returning the inner service
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<S, Request> Service<Request> for InFlightLimit<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = Error<S::Error>;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if self.state.reserved {
            return self.inner.poll_ready().map_err(Error::Upstream);
        }

        self.state.shared.task.register();

        if !self.state.shared.reserve() {
            return Ok(Async::NotReady);
        }

        self.state.reserved = true;

        self.inner.poll_ready().map_err(Error::Upstream)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // In this implementation, `poll_ready` is not expected to be called
        // first (though, it might have been).
        if self.state.reserved {
            self.state.reserved = false;
        } else {
            // Try to reserve
            if !self.state.shared.reserve() {
                panic!("service not ready; call poll_ready first");
            }
        }

        ResponseFuture {
            inner: self.inner.call(request),
            shared: self.state.shared.clone(),
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
        use futures::Async::*;

        match self.inner.poll() {
            Ok(Ready(v)) => Ok(Ready(v)),
            Ok(NotReady) => {
                return Ok(NotReady);
            }
            Err(e) => Err(Error::Upstream(e)),
        }
    }
}

impl<T> Drop for ResponseFuture<T> {
    fn drop(&mut self) {
        self.shared.release();
    }
}

// ===== impl State =====

impl Clone for State {
    fn clone(&self) -> Self {
        State {
            shared: self.shared.clone(),
            reserved: false,
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if self.reserved {
            self.shared.release();
        }
    }
}

// ===== impl Shared =====

impl Shared {
    /// Attempts to reserve capacity for a request. Returns `true` if the
    /// reservation is successful.
    fn reserve(&self) -> bool {
        let mut curr = self.curr.load(SeqCst);

        loop {
            if curr == self.max {
                return false;
            }

            let actual = self.curr.compare_and_swap(curr, curr + 1, SeqCst);

            if actual == curr {
                return true;
            }

            curr = actual;
        }
    }

    /// Release a reserved in-flight request. This is called when either the
    /// request has completed OR the service that made the reservation has
    /// dropped.
    pub fn release(&self) {
        let prev = self.curr.fetch_sub(1, SeqCst);

        // Cannot go above the max number of in-flight
        debug_assert!(prev <= self.max);

        if prev == self.max {
            self.task.notify();
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
            Error::Upstream(ref why) => fmt::Display::fmt(why, f),
        }
    }
}

impl<T> error::Error for Error<T>
where
    T: error::Error,
{
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Upstream(ref why) => Some(why),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Upstream(_) => "upstream service error",
        }
    }
}
