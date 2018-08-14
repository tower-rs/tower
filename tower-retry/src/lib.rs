extern crate futures;
extern crate tower_service;

use futures::{Future, Poll};
use tower_service::Service;

#[derive(Clone, Debug)]
pub struct Retry<P, S> {
    policy: P,
    service: S,
}

#[derive(Debug)]
pub struct ResponseFuture<P, S: Service> {
    attempt: usize,
    future: S::Future,
    request: Option<S::Request>,
    retry: Retry<P, S>,
}

pub trait Policy<R, E> {
    fn retry(&self, err: E, attempt: usize) -> Result<(), E>;
    fn clone_request(&self, req: &R) -> Option<R>;
}


// ===== impl Retry =====

impl<P, S> Retry<P, S>
where
    P: Policy<S::Request, S::Error> + Clone,
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
    P: Policy<S::Request, S::Error> + Clone,
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
            attempt: 1,
            future,
            request: cloned,
            retry: self.clone(),
        }
    }
}

// ===== impl ResponseFuture =====

impl<P, S> Future for ResponseFuture<P, S>
where
    P: Policy<S::Request, S::Error> + Clone,
    S: Service + Clone,
{
    type Item = S::Response;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.future.poll() {
                Ok(v) => return Ok(v),
                Err(err) => {
                    if let Some(req) = self.request.take() {
                        self.retry.policy.retry(err, self.attempt)?;
                        self.attempt += 1;
                        self.request = self.retry.policy.clone_request(&req);
                        self.future = self.retry.service.call(req);
                    } else {
                        // request wasn't cloned, so no way to retry it
                        return Err(err);
                    }
                }
            }
        }
    }
}

