//! Conditionally dispatch requests to the inner service based on the result of
//! a predicate.

extern crate futures;
extern crate tower_service;

use futures::{Future, IntoFuture, Poll, Async};
use futures::task::AtomicTask;
use tower_service::Service;

use std::{fmt, mem};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;

#[derive(Debug)]
pub struct Filter<T, U> {
    inner: T,
    predicate: U,
    // Tracks the number of in-flight requests
    counts: Arc<Counts>,
}

pub struct ResponseFuture<T, S>
where S: Service,
{
    inner: Option<ResponseInner<T, S>>,
}

#[derive(Debug)]
struct ResponseInner<T, S>
where S: Service,
{
    state: State<S::Request, S::Future>,
    check: T,
    service: S,
    counts: Arc<Counts>,
}

/// Errors produced by `Filter`
#[derive(Debug)]
pub enum Error<T, U> {
    /// The predicate rejected the request.
    Rejected(T),

    /// The inner service produced an error.
    Inner(U),

    /// The service is out of capacity.
    NoCapacity,
}

/// Checks a request
pub trait Predicate<T> {
    type Error;
    type Future: Future<Item = (), Error = Self::Error>;

    fn check(&mut self, request: &T) -> Self::Future;
}

#[derive(Debug)]
struct Counts {
    /// Filter::poll_ready task
    task: AtomicTask,

    /// Remaining capacity
    rem: AtomicUsize,
}

#[derive(Debug)]
enum State<T, U> {
    Check(T),
    WaitReady(T),
    WaitResponse(U),
    NoCapacity,
}

// ===== impl Filter =====

impl<T, U> Filter<T, U>
where T: Service + Clone,
      U: Predicate<T::Request>,
{
    pub fn new(inner: T, predicate: U, buffer: usize) -> Self {
        let counts = Counts {
            task: AtomicTask::new(),
            rem: AtomicUsize::new(buffer),
        };

        Filter {
            inner,
            predicate,
            counts: Arc::new(counts),
        }
    }
}

impl<T, U> Service for Filter<T, U>
where T: Service + Clone,
      U: Predicate<T::Request>,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = Error<U::Error, T::Error>;
    type Future = ResponseFuture<U::Future, T>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.counts.task.register();

        let rem = self.counts.rem.load(SeqCst);

        // TODO: Handle catching upstream closing

        if rem == 0 {
            Ok(Async::NotReady)
        } else {
            Ok(().into())
        }
    }

    fn call(&mut self, request: T::Request) -> Self::Future {
        let rem = self.counts.rem.load(SeqCst);

        if rem == 0 {
            return ResponseFuture {
                inner: None,
            };
        }

        // Decrement
        self.counts.rem.fetch_sub(1, SeqCst);

        // Check the request
        let check = self.predicate.check(&request);

        // Clone the service
        let service = self.inner.clone();

        // Clone counts
        let counts = self.counts.clone();

        ResponseFuture {
            inner: Some(ResponseInner {
                state: State::Check(request),
                check,
                service,
                counts,
            }),
        }
    }
}

// ===== impl Predicate =====

impl<F, T, U> Predicate<T> for F
    where F: Fn(&T) -> U,
          U: IntoFuture<Item = ()>,
{
    type Error = U::Error;
    type Future = U::Future;

    fn check(&mut self, request: &T) -> Self::Future {
        self(request).into_future()
    }
}

// ===== impl ResponseFuture =====

impl<T, U> Future for ResponseFuture<T, U>
where T: Future,
      U: Service,
{
    type Item = U::Response;
    type Error = Error<T::Error, U::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.inner {
            Some(ref mut inner) => inner.poll(),
            None => Err(Error::NoCapacity),
        }
    }
}

impl<T, S> fmt::Debug for ResponseFuture<T, S>
where T: fmt::Debug,
      S: Service + fmt::Debug,
      S::Request: fmt::Debug,
      S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ResponseFuture")
            .field("inner", &self.inner)
            .finish()
    }
}

// ===== impl ResponseInner =====

impl<T, U> ResponseInner<T, U>
where T: Future,
      U: Service,
{
    fn inc_rem(&self) {
        if 0 == self.counts.rem.fetch_add(1, SeqCst) {
            self.counts.task.notify();
        }
    }

    fn poll(&mut self) -> Poll<U::Response, Error<T::Error, U::Error>> {
        use self::State::*;

        loop {
            match mem::replace(&mut self.state, NoCapacity) {
                Check(request) => {
                    // Poll predicate
                    match self.check.poll() {
                        Ok(Async::Ready(_)) => {
                            self.state = WaitReady(request);
                        }
                        Ok(Async::NotReady) => {
                            self.state = Check(request);
                            return Ok(Async::NotReady);
                        }
                        Err(e) => {
                            return Err(Error::Rejected(e));
                        }
                    }
                }
                WaitReady(request) => {
                    // Poll service for readiness
                    match self.service.poll_ready() {
                        Ok(Async::Ready(_)) => {
                            self.inc_rem();

                            let response = self.service.call(request);
                            self.state = WaitResponse(response);
                        }
                        Ok(Async::NotReady) => {
                            self.state = WaitReady(request);
                            return Ok(Async::NotReady);
                        }
                        Err(e) => {
                            self.inc_rem();

                            return Err(Error::Inner(e));
                        }
                    }
                }
                WaitResponse(mut response) => {
                    let ret = response.poll()
                        .map_err(Error::Inner);

                    self.state = WaitResponse(response);

                    return ret;
                }
                NoCapacity => {
                    return Err(Error::NoCapacity);
                }
            }
        }
    }
}
