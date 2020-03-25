use super::Rate;
use futures_core::ready;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::time::{Delay, Instant};
use tower_service::Service;

/// Enforces a rate limit on the number of requests the underlying
/// service can handle over a period of time.
#[derive(Debug)]
pub struct RateLimit<T> {
    inner: T,
    rate: Rate,
    state: State,
}

#[derive(Debug)]
enum State {
    // The service has hit its limit
    Limited(Delay),
    Ready { until: Instant, rem: u64 },
}

impl<T> RateLimit<T> {
    /// Create a new rate limiter
    pub fn new(inner: T, rate: Rate) -> Self {
        let state = State::Ready {
            until: Instant::now(),
            rem: rate.num(),
        };

        RateLimit {
            inner,
            rate,
            state: state,
        }
    }

    /// Get a reference to the inner service
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    /// Get a mutable reference to the inner service
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Consume `self`, returning the inner service
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<S, Request> Service<Request> for RateLimit<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.state {
            State::Ready { .. } => return Poll::Ready(ready!(self.inner.poll_ready(cx))),
            State::Limited(ref mut sleep) => {
                ready!(Pin::new(sleep).poll(cx));
            }
        }

        self.state = State::Ready {
            until: Instant::now() + self.rate.per(),
            rem: self.rate.num(),
        };

        Poll::Ready(ready!(self.inner.poll_ready(cx)))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        match self.state {
            State::Ready { mut until, mut rem } => {
                let now = Instant::now();

                // If the period has elapsed, reset it.
                if now >= until {
                    until = now + self.rate.per();
                    let rem = self.rate.num();

                    self.state = State::Ready { until, rem }
                }

                if rem > 1 {
                    rem -= 1;
                    self.state = State::Ready { until, rem };
                } else {
                    // The service is disabled until further notice
                    let sleep = tokio::time::delay_until(until);
                    self.state = State::Limited(sleep);
                }

                // Call the inner future
                self.inner.call(request)
            }
            State::Limited(..) => panic!("service not ready; poll_ready must be called first"),
        }
    }
}

impl<S> tower_load::Load for RateLimit<S>
where
    S: tower_load::Load,
{
    type Metric = S::Metric;
    fn load(&self) -> Self::Metric {
        self.inner.load()
    }
}
