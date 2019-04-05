use futures::{Async, Future, Poll};
use tokio_timer::clock;
use tower_service::Service;

use std::time::{Duration, Instant};

/// Record is the interface for accepting request latency measurements.  When
/// a request completes, record is called with the elapsed duration between
/// when the service was called and when the future completed.
pub trait Record {
    fn record(&mut self, latency: Duration);
}

/// Latency is a middleware that measures request latency and records it to the
/// provided Record instance.
#[derive(Clone)]
pub struct Latency<R, S> {
    rec: R,
    service: S,
}

pub struct ResponseFuture<R, F> {
    start: Instant,
    rec: R,
    inner: F,
}

impl<S, R> Latency<R, S>
where
    R: Record + Clone,
{
    pub fn new<Request>(rec: R, service: S) -> Self
    where
        S: Service<Request>,
        S::Error: Into<super::Error>,
    {
        Latency { rec, service }
    }
}

impl<S, R, Request> Service<Request> for Latency<R, S>
where
    S: Service<Request>,
    S::Error: Into<super::Error>,
    R: Record + Clone,
{
    type Response = S::Response;
    type Error = super::Error;
    type Future = ResponseFuture<R, S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready().map_err(|e| e.into())
    }

    fn call(&mut self, request: Request) -> Self::Future {
        ResponseFuture {
            start: clock::now(),
            rec: self.rec.clone(),
            inner: self.service.call(request),
        }
    }
}

impl<R, F> Future for ResponseFuture<R, F>
where
    R: Record,
    F: Future,
    F::Error: Into<super::Error>,
{
    type Item = F::Item;
    type Error = super::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.inner.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(rsp)) => {
                let duration = clock::now() - self.start;
                self.rec.record(duration);
                Ok(Async::Ready(rsp))
            }
            Err(e) => Err(e.into()),
        }
    }
}
