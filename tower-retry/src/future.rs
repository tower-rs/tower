//! Future types

use crate::{Policy, Retry};
use futures::{try_ready, Async, Future, Poll};
use tower_service::Service;

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

impl<P, S, Request> ResponseFuture<P, S, Request>
where
    P: Policy<Request, S::Response, S::Error>,
    S: Service<Request>,
{
    pub(crate) fn new(
        request: Option<Request>,
        retry: Retry<P, S>,
        future: S::Future,
    ) -> ResponseFuture<P, S, Request> {
        ResponseFuture {
            request,
            retry,
            state: State::Called(future),
        }
    }
}

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
