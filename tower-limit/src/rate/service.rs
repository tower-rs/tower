use super::{error::Error, future::ResponseFuture, Rate};
use futures::{Async, Future, Poll};
use tokio_timer::{clock, Delay};
use tower_service::Service;

use std::time::Instant;

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
            until: clock::now(),
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
    Error: From<S::Error>,
{
    type Response = S::Response;
    type Error = Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        match self.state {
            State::Ready { .. } => return self.inner.poll_ready().map_err(Into::into),
            State::Limited(ref mut sleep) => {
                if let Async::NotReady = sleep.poll()? {
                    tracing::trace!("rate limited exceeded; sleeping.");
                }
            }
        }

        self.state = State::Ready {
            until: clock::now() + self.rate.per(),
            rem: self.rate.num(),
        };

        self.inner.poll_ready().map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        match self.state {
            State::Ready { mut until, mut rem } => {
                let now = clock::now();

                // If the period has elapsed, reset it.
                if now >= until {
                    until = now + self.rate.per();
                    rem = self.rate.num();
                }

                if rem > 1 {
                    rem -= 1;
                    self.state = State::Ready { until, rem };
                } else {
                    let sleep = Delay::new(until);
                    self.state = State::Limited(sleep);
                }

                // Call the inner future
                let inner = self.inner.call(request);
                ResponseFuture::new(inner)
            }
            State::Limited(..) => panic!("service not ready; poll_ready must be called first"),
        }
    }
}
