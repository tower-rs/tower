//! Tower middleware that limits the maximum number of in-flight requests for a
//! service.

extern crate futures;
extern crate tower_service;

use tower_service::Service;

use futures::{Future, Poll, Async};
use futures::task::AtomicTask;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;

#[derive(Debug, Clone)]
pub struct InFlightLimit<T> {
    inner: T,
    state: State,
}

#[derive(Debug)]
pub struct ResponseFuture<T> {
    inner: Option<T>,
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

#[macro_use]
mod macros {
    include! { concat!(env!("CARGO_MANIFEST_DIR"), "/../gen_errors.rs") }
}

kind_error!{
    /// The request has been rate limited
    ///
    /// TODO: Consider returning the original request
    #[derive(Debug)]
    pub struct Error from enum ErrorKind {
        NoCapacity => fmt: "in-flight limit exceeded", is: is_at_capacity, into: UNUSED, borrow: UNUSED,
        Upstream(T) => is: is_upstream, into: into_upstream, borrow: borrow_upstream
    }
}

// ===== impl InFlightLimit =====

impl<T> InFlightLimit<T> {
    /// Create a new rate limiter
    pub fn new(inner: T, max: usize) -> Self {
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

impl<S> Service for InFlightLimit<S>
where S: Service
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = Error<S::Error>;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if self.state.reserved {
            return self.inner.poll_ready()
                .map_err(|e| ErrorKind::Upstream(e).into());
        }

        self.state.shared.task.register();

        if !self.state.shared.reserve() {
            return Ok(Async::NotReady);
        }

        self.state.reserved = true;

        self.inner.poll_ready()
            .map_err(|e| ErrorKind::Upstream(e).into())
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        // In this implementation, `poll_ready` is not expected to be called
        // first (though, it might have been).
        if self.state.reserved {
            self.state.reserved = false;
        } else {
            // Try to reserve
            if !self.state.shared.reserve() {
                return ResponseFuture {
                    inner: None,
                    shared: self.state.shared.clone(),
                };
            }
        }

        ResponseFuture {
            inner: Some(self.inner.call(request)),
            shared: self.state.shared.clone(),
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
        use futures::Async::*;

        let res = match self.inner {
            Some(ref mut f) => {
                match f.poll() {
                    Ok(Ready(v)) => {
                        self.shared.release();
                        Ok(Ready(v))
                    }
                    Ok(NotReady) => {
                        return Ok(NotReady);
                    }
                    Err(e) => {
                        self.shared.release();
                        Err(ErrorKind::Upstream(e).into())
                    }
                }
            }
            None => Err(ErrorKind::NoCapacity.into()),
        };

        // Drop the inner future
        self.inner = None;

        res
    }
}

impl<T> Drop for ResponseFuture<T> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            self.shared.release();
        }
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
