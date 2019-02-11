//! Conditionally dispatch requests to the inner service based on the result of
//! a predicate.

extern crate futures;
extern crate tower_service;

use futures::task::AtomicTask;
use futures::{Async, Future, IntoFuture, Poll};
use tower_service::Service;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::{error::Error as StdError, fmt, mem};

#[derive(Debug)]
pub struct Filter<T, U> {
    inner: T,
    predicate: U,
    // Tracks the number of in-flight requests
    counts: Arc<Counts>,
}

pub struct ResponseFuture<T, S, Request>
where
    S: Service<Request>,
{
    inner: Option<ResponseInner<T, S, Request>>,
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

type Error = Box<StdError + Send + Sync>;

/// The predicate rejected the request.
#[derive(Debug)]
struct RejectedError(Error);

impl StdError for RejectedError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self)
    }
}

impl fmt::Display for RejectedError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Predicate rejected the request")
    }
}

/// The service is out of capacity.
#[derive(Debug)]
struct NoCapacityError;

impl StdError for NoCapacityError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self)
    }
}

impl fmt::Display for NoCapacityError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Service is at capacity")
    }
}

/// Checks a request
pub trait Predicate<Request> {
    type Error;
    type Future: Future<Item = (), Error = Self::Error>;

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
    NoCapacity,
}

// ===== impl Filter =====

impl<T, U> Filter<T, U> {
    pub fn new<Request>(inner: T, predicate: U, buffer: usize) -> Self
    where
        T: Service<Request> + Clone,
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
    T::Error: Into<Error>,
    U: Predicate<Request>,
    <U as Predicate<Request>>::Error: Into<Error>,
    <T as Service<Request>>::Error: Into<Error>,
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
            return ResponseFuture { inner: None };
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
where
    F: Fn(&T) -> U,
    U: IntoFuture<Item = ()>,
{
    type Error = U::Error;
    type Future = U::Future;

    fn check(&mut self, request: &T) -> Self::Future {
        self(request).into_future()
    }
}

// ===== impl ResponseFuture =====

impl<T, S, Request> Future for ResponseFuture<T, S, Request>
where
    T: Future,
    T::Error: Into<Error>,
    S: Service<Request>,
    <S as Service<Request>>::Error: Into<Error>,
{
    type Item = S::Response;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.inner {
            Some(ref mut inner) => inner.poll(),
            None => Err(NoCapacityError.into()),
        }
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
    T: Future,
    T::Error: Into<Error>,
    S: Service<Request>,
    <S as Service<Request>>::Error: Into<Error>,
{
    fn inc_rem(&self) {
        if 0 == self.counts.rem.fetch_add(1, SeqCst) {
            self.counts.task.notify();
        }
    }

    fn poll(&mut self) -> Poll<S::Response, Error> {
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
                            return Err(RejectedError(e.into()).into());
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

                            return Err(e.into());
                        }
                    }
                }
                WaitResponse(mut response) => {
                    let ret = response.poll().map_err(|e| e.into());

                    self.state = WaitResponse(response);

                    return ret;
                }
                NoCapacity => {
                    return Err(NoCapacityError.into());
                }
            }
        }
    }
}
