#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![deny(warnings)]

//! Tower middleware for retrying "failed" requests.

#[macro_use]
extern crate futures;
extern crate tokio_timer;
extern crate tower_service;

use oneshot::Oneshot;
use futures::{Async, Future, Poll};
use tower_service::Service;
use std::fmt;
use std::mem;

pub mod budget;
mod oneshot;

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
    type Future: Future<Item=Self, Error=()>;
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

/// The `Future` returned by a `Retry` service.
pub struct ResponseFuture<P, S, Request>
where
    P: Policy<Request, S::Response, S::Error> + Clone,
    S: Service<Request> + Clone,
{
    request: Option<Request>,
    state: State<P, S, Request>,
}

enum State<P, S, Request>
where
    P: Policy<Request, S::Response, S::Error> + Clone,
    S: Service<Request> + Clone,
{
    /// Polling the future from `Service::call`
    Called(P, Oneshot<S, Request>),
    /// Polling the future from `Policy::retry`
    Checking(P::Future, Option<Result<S::Response, S::Error>>, Retry<P, S>),
    /// Polling `Service::poll_ready` after `Checking` was OK.
    Retrying(Retry<P, S>),
    /// Transient state
    Invalid,
}

// ===== impl Retry =====

impl<P, S> Retry<P, S> {
    /// Retry the inner service depending on this [`Policy`][Policy}.
    pub fn new<Request>(policy: P, service: S) -> Self
    where
        P: Policy<Request, S::Response, S::Error> + Clone,
        S: Service<Request> + Clone,
    {
        Retry {
            policy,
            service,
        }
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
        let policy = self.policy.clone();
        let oneshot = Oneshot::new(future, self.service.clone());

        ResponseFuture {
            request: cloned,
            state: State::Called(policy, oneshot),
        }
    }

    fn poll_service(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_service()
    }

    fn poll_close(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_close()
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
            match self.state {
                State::Called(ref policy, ref mut future) => {
                    let result = match future.poll() {
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Ok(Async::Ready(res)) => Ok(res),
                        Err(err) => Err(err),
                    };

                    if let Some(ref req) = self.request {
                        match policy.retry(req, result.as_ref()) {
                            Some(checking) => {
                                self.state.to_checking(
                                    checking,
                                    result);
                            }
                            None => return result.map(Async::Ready),
                        }
                    } else {
                        // request wasn't cloned, so no way to retry it
                        return result.map(Async::Ready);
                    }
                },
                State::Checking(ref mut future, ref mut result, _) => {
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

                    let mut svc = self.state.unwrap_checking();
                    svc.policy = policy;

                    self.state = State::Retrying(svc);
                },
                State::Retrying(ref mut svc) => {
                    try_ready!(svc.poll_ready());

                    let req = self
                        .request
                        .take()
                        .expect("retrying requires cloned request");

                    let mut svc = self.state.unwrap_retrying();

                    // Store the request for future usage
                    self.request = svc.policy.clone_request(&req);

                    let future = svc.service.call(req);
                    let oneshot = Oneshot::new(future, svc.service);

                    self.state = State::Called(svc.policy, oneshot)
                }
                _ => panic!(),
            }
        }
    }
}

impl<P, S, Request> fmt::Debug for ResponseFuture<P, S, Request>
where
    Request: fmt::Debug,
    P: Policy<Request, S::Response, S::Error> + Clone + fmt::Debug,
    P::Future: fmt::Debug,
    S: Service<Request> + Clone + fmt::Debug,
    S::Response: fmt::Debug,
    S::Error: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ResponseFuture")
            .field("request", &self.request)
            .field("state", &self.state)
            .finish()
    }
}

impl<P, S, Request> State<P, S, Request>
where
    P: Policy<Request, S::Response, S::Error> + Clone,
    S: Service<Request> + Clone,
{
    fn to_checking(
        &mut self,
        checking: P::Future,
        res: Result<S::Response, S::Error>
    ) {
        match mem::replace(self, State::Invalid) {
            State::Called(policy, oneshot) => {
                let service = oneshot.into_service();

                *self = State::Checking(
                    checking,
                    Some(res),
                    Retry {
                        policy,
                        service,
                    });
            }
            _ => panic!(),
        }
    }

    fn unwrap_checking(&mut self) -> Retry<P, S> {
        match mem::replace(self, State::Invalid) {
            State::Checking(_, _, svc) => svc,
            _ => panic!(),
        }
    }

    fn unwrap_retrying(&mut self) -> Retry<P, S> {
        match mem::replace(self, State::Invalid) {
            State::Retrying(svc) => svc,
            _ => panic!(),
        }
    }
}

impl<P, S, Request> fmt::Debug for State<P, S, Request>
where
    Request: fmt::Debug,
    P: Policy<Request, S::Response, S::Error> + Clone + fmt::Debug,
    P::Future: fmt::Debug,
    S: Service<Request> + Clone + fmt::Debug,
    S::Response: fmt::Debug,
    S::Error: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use self::State::*;

        match *self {
            Called(ref a, ref b) => {
                fmt.debug_tuple("State::Called")
                    .field(a)
                    .field(b)
                    .finish()
            }
            Checking(ref a, ref b, ref c) => {
                fmt.debug_tuple("State::Checking")
                    .field(a)
                    .field(b)
                    .field(c)
                    .finish()
            }
            Retrying(ref a) => {
                fmt.debug_tuple("State::Retrying")
                    .field(a)
                    .finish()
            }
            Invalid => {
                fmt.debug_tuple("State::Invalid")
                    .finish()
            }
        }
    }
}
