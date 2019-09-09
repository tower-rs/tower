//! Future types

use crate::{Policy, Retry};
use futures_core::ready;
use pin_project::{pin_project, project};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower_service::Service;

/// The `Future` returned by a `Retry` service.
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<P, S, Request>
where
    P: Policy<Request, S::Response, S::Error>,
    S: Service<Request>,
{
    request: Option<Request>,
    retry: Retry<P, S>,
    #[pin]
    state: State<S::Future, P::Future, S::Response, S::Error>,
}

#[pin_project]
#[derive(Debug)]
enum State<F, P, R, E> {
    /// Polling the future from `Service::call`
    Called(#[pin] F),
    /// Polling the future from `Policy::retry`
    Checking(#[pin] P, Option<Result<R, E>>),
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
    P: Policy<Request, S::Response, S::Error> + Clone + Unpin,
    S: Service<Request> + Clone,
{
    type Output = Result<S::Response, S::Error>;

    #[project]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            #[project]
            match this.state.project() {
                State::Called(future) => {
                    let result = ready!(future.poll(cx));
                    if let Some(ref req) = this.request {
                        match this.retry.policy.retry(req, result.as_ref()) {
                            Some(checking) => {
                                this.state.set(State::Checking(checking, Some(result)));
                            }
                            None => return Poll::Ready(result),
                        }
                    } else {
                        // request wasn't cloned, so no way to retry it
                        return Poll::Ready(result);
                    }
                }
                State::Checking(future, result) => {
                    let policy = match ready!(future.poll(cx)) {
                        Some(policy) => policy,
                        None => {
                            // if Policy::retry() fails, return the original
                            // result...
                            return Poll::Ready(result.take().expect("polled after complete"));
                        }
                    };
                    this.retry.policy = policy;
                    this.state.set(State::Retrying);
                }
                State::Retrying => {
                    ready!(this.retry.poll_ready(cx))?;
                    let req = this
                        .request
                        .take()
                        .expect("retrying requires cloned request");
                    *this.request = this.retry.policy.clone_request(&req);
                    this.state.set(State::Called(this.retry.service.call(req)));
                }
            }
        }
    }
}
