#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
// #![deny(warnings)]
#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]

//! Tower middleware for retrying "failed" requests.

use futures::{try_ready, Async, Future, Poll};
use tower_layer::Layer;
use tower_service::Service;

pub mod budget;

/// A "retry policy" to classify if a request should be retried.
///
/// # Example
///
/// ```
/// extern crate futures;
/// extern crate tower_retry;
///
/// use tower_retry::Policy;
///
/// type Req = String;
/// type Res = String;
///
/// struct Attempts(usize);
///
/// impl<E> Policy<Req, Res, E> for Attempts {
///     type Future = futures::future::FutureResult<Self, ()>;
///
///     fn retry(&self, req: &Req, result: Result<&Res, &E>) -> Option<Self::Future> {
///         match result {
///             Ok(_) => {
///                 // Treat all `Response`s as success,
///                 // so don't retry...
///                 None
///             },
///             Err(_) => {
///                 // Treat all errors as failures...
///                 // But we limit the number of attempts...
///                 if self.0 > 0 {
///                     // Try again!
///                     Some(futures::future::ok(Attempts(self.0 - 1)))
///                 } else {
///                     // Used all our attempts, no retry...
///                     None
///                 }
///             }
///         }
///     }
///
///     fn clone_request(&self, req: &Req) -> Option<Req> {
///         Some(req.clone())
///     }
/// }
/// ```
pub trait Policy<Req, Res, E>: Sized {
    /// The `Future` type returned by `Policy::retry()`.
    type Future: Future<Item = Self, Error = ()>;
    /// Check the policy if a certain request should be retried.
    ///
    /// This method is passed a reference to the original request, and either
    /// the `Service::Response` or `Service::Error` from the inner service.
    ///
    /// If the request should **not** be retried, return `None`.
    ///
    /// If the request *should* be retried, return `Some` future of a new
    /// policy that would apply for the next request attempt.
    ///
    /// If the returned `Future` errors, the request will **not** be retried
    /// after all.
    fn retry(&self, req: &Req, result: Result<&Res, &E>) -> Option<Self::Future>;
    /// Tries to clone a request before being passed to the inner service.
    ///
    /// If the request cannot be cloned, return `None`.
    fn clone_request(&self, req: &Req) -> Option<Req>;
}

/// Configure retrying requests of "failed" responses.
///
/// A `Policy` classifies what is a "failed" response.
#[derive(Clone, Debug)]
pub struct Retry<P, S> {
    policy: P,
    service: S,
}

/// Retry requests based on a policy
#[derive(Debug)]
pub struct RetryLayer<P> {
    policy: P,
}

/// The `Future` returned by a `Retry` service.
#[derive(Debug)]
pub struct ResponseFuture<P, S, Request>
where
    P: Policy<Request, S::Response, S::Error>,
    S: Service<Request>,
{
    request: Option<Request>,
    retry: Retry<P, S>,
    state: State<S::Future, P::Future, S::Response, S::Error>,
}

#[derive(Debug)]
enum State<F, P, R, E> {
    /// Polling the future from `Service::call`
    Called(F),
    /// Polling the future from `Policy::retry`
    Checking(P, Option<Result<R, E>>),
    /// Polling `Service::poll_ready` after `Checking` was OK.
    Retrying,
}

// ===== impl RetryLayer =====

impl<P> RetryLayer<P> {
    /// Create a new `RetryLayer` from a retry policy
    pub fn new(policy: P) -> Self {
        RetryLayer { policy }
    }
}

impl<P, S> Layer<S> for RetryLayer<P>
where
    P: Clone,
{
    type Service = Retry<P, S>;

    fn layer(&self, service: S) -> Self::Service {
        let policy = self.policy.clone();
        Retry::new(policy, service)
    }
}

// ===== impl Retry =====

impl<P, S> Retry<P, S> {
    /// Retry the inner service depending on this [`Policy`][Policy}.
    pub fn new(policy: P, service: S) -> Self {
        Retry { policy, service }
    }
}

impl<P, S, Request> Service<Request> for Retry<P, S>
where
    P: Policy<Request, S::Response, S::Error> + Clone,
    S: Service<Request> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<P, S, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let cloned = self.policy.clone_request(&request);
        let future = self.service.call(request);
        ResponseFuture {
            request: cloned,
            retry: self.clone(),
            state: State::Called(future),
        }
    }
}

// ===== impl ResponseFuture =====

impl<P, S, Request> Future for ResponseFuture<P, S, Request>
where
    P: Policy<Request, S::Response, S::Error> + Clone,
    S: Service<Request> + Clone,
{
    type Item = S::Response;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let next = match self.state {
                State::Called(ref mut future) => {
                    let result = match future.poll() {
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Ok(Async::Ready(res)) => Ok(res),
                        Err(err) => Err(err),
                    };

                    if let Some(ref req) = self.request {
                        match self.retry.policy.retry(req, result.as_ref()) {
                            Some(checking) => State::Checking(checking, Some(result)),
                            None => return result.map(Async::Ready),
                        }
                    } else {
                        // request wasn't cloned, so no way to retry it
                        return result.map(Async::Ready);
                    }
                }
                State::Checking(ref mut future, ref mut result) => {
                    let policy = match future.poll() {
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Ok(Async::Ready(policy)) => policy,
                        Err(()) => {
                            // if Policy::retry() fails, return the original
                            // result...
                            return result
                                .take()
                                .expect("polled after complete")
                                .map(Async::Ready);
                        }
                    };
                    self.retry.policy = policy;
                    State::Retrying
                }
                State::Retrying => {
                    try_ready!(self.retry.poll_ready());
                    let req = self
                        .request
                        .take()
                        .expect("retrying requires cloned request");
                    self.request = self.retry.policy.clone_request(&req);
                    State::Called(self.retry.service.call(req))
                }
            };
            self.state = next;
        }
    }
}
