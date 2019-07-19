//! A middleware that blocks readiness based on an inner service's load.

pub mod error;

use futures::{future, try_ready, Future, Poll};
use tower_load::Load;
use tower_service as svc;

/// Determmines readiness based on an `M`-typed load metric.
pub trait Breaker<M> {
    /// Checks to see whether the breaker is open, given a load metric.
    ///
    /// If the breaker is closed, NotReady is returned and the task ust be
    /// notified when the breaker should be polled again with an updated load
    /// metric.
    fn poll_breaker(&mut self, load: M) -> Poll<(), error::Error>;
}

/// Wraps a load-bearing service with a load-dependent circuit-breaker.
pub struct Service<B, S> {
    breaker: B,
    inner: S,
}

// === impl CircuitBreaker ===

impl<B, S> Service<B, S> {
    /// Wraps an `S` typed service with `B`-typed breaker.
    pub fn new(breaker: B, inner: S) -> Self {
        Self { breaker, inner }
    }
}

impl<B, S, Req> svc::Service<Req> for Service<B, S>
where
    B: Breaker<S::Metric>,
    S: Load + svc::Service<Req>,
    S::Error: Into<error::Error>,
{
    type Response = S::Response;
    type Error = error::Error;
    type Future = future::MapErr<S::Future, fn(S::Error) -> error::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        try_ready!(self.inner.poll_ready().map_err(Into::into));

        // Update the breaker with the current load and only advertise readiness
        // when the breaker is open.
        self.breaker.poll_breaker(self.inner.load())
    }

    fn call(&mut self, req: Req) -> Self::Future {
        self.inner.call(req).map_err(Into::into)
    }
}

#[allow(missing_docs)]
pub mod delay {
    use crate::error;
    use futures::{try_ready, Async, Future, Poll};
    use std::time::Instant;
    use tokio_timer::Delay;

    pub trait BreakUntil<M> {
        fn break_until(&mut self, load: M) -> Option<Instant>;
    }

    pub struct Breaker<B> {
        break_until: B,
        delay: Option<Delay>,
    }

    impl<M, B: BreakUntil<M>> super::Breaker<M> for Breaker<B> {
        fn poll_breaker(&mut self, load: M) -> Poll<(), error::Error> {
            // Even if there's already a delay, update the inner breaker with
            // the new load and reset any pre-existing delay.
            //
            // The `delay` is stored so that the breaker task is notified after
            // the delay expires.
            self.delay = self.break_until.break_until(load).map(Delay::new);

            if let Some(delay) = self.delay.as_mut() {
                try_ready!(delay.poll());
                self.delay = None;
            }

            Ok(Async::Ready(()))
        }
    }

    #[allow(unused_imports)]
    pub mod peak_ewma {
        use tower_load::peak_ewma;

        pub struct Breaker {}
    }
}
