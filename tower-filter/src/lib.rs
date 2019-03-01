//! Conditionally dispatch requests to the inner service based on the result of
//! a predicate.

extern crate futures;
extern crate tower_layer;
extern crate tower_service;

pub mod error;
pub mod future;

use error::Error;
use futures::task::AtomicTask;
use futures::{Async, Future, IntoFuture, Poll};
use tower_layer::Layer;
use tower_service::Service;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::{fmt, mem};

#[derive(Debug)]
pub struct Filter<T, U> {
    inner: T,
    predicate: U,
    // Tracks the number of in-flight requests
    counts: Arc<Counts>,
}

pub struct FilterLayer<U> {
    predicate: U,
    buffer: usize,
}

pub struct ResponseFuture<T, S, Request>
where
    S: Service<Request>,
{
    inner: ResponseInner<T, S, Request>,
}

#[derive(Debug)]
struct ResponseInner<T, S, Request>
where
    S: Service<Request>,
{
    state: State<Request, S::Future>,
    check: T,
    service: S,
    counts: Arc<Counts>,
}

/// Checks a request
pub trait Predicate<Request> {
    type Future: Future<Item = (), Error = error::Error>;

    fn check(&mut self, request: &Request) -> Self::Future;
}

#[derive(Debug)]
struct Counts {
    /// Filter::poll_ready task
    task: AtomicTask,

    /// Remaining capacity
    rem: AtomicUsize,
}

#[derive(Debug)]
enum State<Request, U> {
    Check(Request),
    WaitReady(Request),
    WaitResponse(U),
    Invalid,
}

// ===== impl Filter =====

impl<U> FilterLayer<U> {
    pub fn new(predicate: U, buffer: usize) -> Self {
        FilterLayer { predicate, buffer }
    }
}

impl<U, S, Request> Layer<S, Request> for FilterLayer<U>
where
    U: Predicate<Request> + Clone,
    S: Service<Request> + Clone,
    S::Error: Into<error::Source>,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = error::never::Never;
    type Service = Filter<S, U>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        let predicate = self.predicate.clone();
        Ok(Filter::new(service, predicate, self.buffer))
    }
}

// ===== impl Filter =====

impl<T, U> Filter<T, U> {
    pub fn new<Request>(inner: T, predicate: U, buffer: usize) -> Self
    where
        T: Service<Request> + Clone,
        T::Error: Into<error::Source>,
        U: Predicate<Request>,
    {
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
        self.counts.task.register();

        let rem = self.counts.rem.load(SeqCst);

        // TODO: Handle catching upstream closing

        if rem == 0 {
            Ok(Async::NotReady)
        } else {
            Ok(().into())
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let rem = self.counts.rem.load(SeqCst);

        if rem == 0 {
            panic!("service not ready; poll_ready must be called first");
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
            inner: ResponseInner {
                state: State::Check(request),
                check,
                service,
                counts,
            },
        }
    }
}

// ===== impl Predicate =====

impl<F, T, U> Predicate<T> for F
where
    F: Fn(&T) -> U,
    U: IntoFuture<Item = (), Error = Error>,
{
    type Future = U::Future;

    fn check(&mut self, request: &T) -> Self::Future {
        self(request).into_future()
    }
}

// ===== impl ResponseFuture =====

impl<T, S, Request> Future for ResponseFuture<T, S, Request>
where
    T: Future<Error = error::Error>,
    S: Service<Request>,
    S::Error: Into<error::Source>,
{
    type Item = S::Response;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll()
    }
}

impl<T, S, Request> fmt::Debug for ResponseFuture<T, S, Request>
where
    T: fmt::Debug,
    S: Service<Request> + fmt::Debug,
    S::Future: fmt::Debug,
    Request: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ResponseFuture")
            .field("inner", &self.inner)
            .finish()
    }
}

// ===== impl ResponseInner =====

impl<T, S, Request> ResponseInner<T, S, Request>
where
    T: Future<Error = error::Error>,
    S: Service<Request>,
    S::Error: Into<error::Source>,
{
    fn inc_rem(&self) {
        if 0 == self.counts.rem.fetch_add(1, SeqCst) {
            self.counts.task.notify();
        }
    }

    fn poll(&mut self) -> Poll<S::Response, Error> {
        use self::State::*;

        loop {
            match mem::replace(&mut self.state, Invalid) {
                Check(request) => {
                    // Poll predicate
                    match self.check.poll()? {
                        Async::Ready(_) => {
                            self.state = WaitReady(request);
                        }
                        Async::NotReady => {
                            self.state = Check(request);
                            return Ok(Async::NotReady);
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

                            return Err(Error::inner(e));
                        }
                    }
                }
                WaitResponse(mut response) => {
                    let ret = response.poll()
                        .map_err(Error::inner);

                    self.state = WaitResponse(response);

                    return ret;
                }
                Invalid => {
                    panic!("invalid state");
                }
            }
        }
    }
}
