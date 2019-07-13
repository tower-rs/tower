#![allow(missing_docs)]

use crate::error;
use futures::{future, try_ready, Async, Future, Poll};
use tower_load::Load;
use tower_service::Service;

pub trait Breaker<M> {
    type Future: Future<Item = (), Error = error::Error>;

    fn close_for(&self, load: M) -> Option<Self::Future>;
}

pub struct CicruitBreaker<B, C, S> {
    breaker: B,
    closed: Option<C>,
    inner: S,
}

impl<B, S, Req> Service<Req> for CicruitBreaker<B, B::Future, S>
where
    B: Breaker<S::Metric>,
    S: Load + Service<Req>,
    S::Error: Into<error::Error>,
{
    type Response = S::Response;
    type Error = error::Error;
    type Future = future::MapErr<S::Future, fn(S::Error) -> error::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // Poll the inner service first so that errors break through.
        let inner_ready = self.inner.poll_ready().map_err(Into::into)?;

        // If a delay is already active, don't proceed until it completes.
        if let Some(ref mut fut) = self.closed.as_mut() {
            try_ready!(fut.poll());
        }
        self.closed = None;

        // Determine if the breaker should close given the current inner service
        // load. If it should close, wait until it reopens..
        if let Some(mut fut) = self.breaker.close_for(self.inner.load()) {
            if fut.poll()?.is_not_ready() {
                self.closed = Some(fut);
                return Ok(Async::NotReady);
            }
        }

        // Finally, when the service is not closed, defer to the inner service's
        // state.
        Ok(inner_ready)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        self.inner.call(req).map_err(Into::into)
    }
}
