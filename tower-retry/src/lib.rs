#[macro_use]
extern crate futures;
extern crate tower_service;

use futures::{Async, Future, Poll};
use tower_service::Service;

#[derive(Clone, Debug)]
pub struct Retry<P, S> {
    policy: P,
    service: S,
}

#[derive(Debug)]
pub struct ResponseFuture<P: Policy<S::Request, S::Response, S::Error>, S: Service> {
    request: Option<S::Request>,
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

pub trait Policy<Req, Res, E>: Sized {
    type Future: Future<Item=Self, Error=()>;
    fn retry(&self, req: &Req, res: Result<&Res, &E>) -> Option<Self::Future>;
    fn clone_request(&self, req: &Req) -> Option<Req>;
}


// ===== impl Retry =====

impl<P, S> Retry<P, S>
where
    P: Policy<S::Request, S::Response, S::Error> + Clone,
    S: Service + Clone,
{
    pub fn new(policy: P, service: S) -> Self {
        Retry {
            policy,
            service,
        }
    }
}

impl<P, S> Service for Retry<P, S>
where
    P: Policy<S::Request, S::Response, S::Error> + Clone,
    S: Service + Clone,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<P, S>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
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

impl<P, S> Future for ResponseFuture<P, S>
where
    P: Policy<S::Request, S::Response, S::Error> + Clone,
    S: Service + Clone,
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
                },
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
                },
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

