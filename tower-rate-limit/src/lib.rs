//! A Tower middleware that rate limits the requests that are passed to the
//! inner service.

#[macro_use]
extern crate futures;
extern crate tokio_timer;
extern crate tower_layer;
extern crate tower_service;

use futures::{Future, Poll};
use tokio_timer::Delay;
use tower_layer::Layer;
use tower_service::Service;

use std::time::{Duration, Instant};
use std::{error, fmt};

#[derive(Debug)]
pub struct RateLimit<T> {
    inner: T,
    rate: Rate,
    state: State,
}

#[derive(Debug)]
pub struct RateLimitLayer {
    rate: Rate,
}

#[derive(Debug, Copy, Clone)]
pub struct Rate {
    num: u64,
    per: Duration,
}

/// The request has been rate limited
///
/// TODO: Consider returning the original request
#[derive(Debug)]
pub enum Error<T> {
    RateLimit,
    Upstream(T),
}

pub struct ResponseFuture<T> {
    inner: T,
}

#[derive(Debug)]
enum State {
    // The service has hit its limit
    Limited(Delay),
    Ready { until: Instant, rem: u64 },
}

impl RateLimitLayer {
    pub fn new(num: u64, per: Duration) -> Self {
        let rate = Rate { num, per };
        RateLimitLayer { rate }
    }
}

impl<S, Request> Layer<S, Request> for RateLimitLayer
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = Error<S::Error>;
    type Service = RateLimit<S>;

    fn wrap(&self, service: S) -> Self::Service {
        RateLimit::new(service, self.rate)
    }
}

impl<T> RateLimit<T> {
    /// Create a new rate limiter
    pub fn new<Request>(inner: T, rate: Rate) -> Self
    where
        T: Service<Request>,
    {
        let state = State::Ready {
            until: Instant::now(),
            rem: rate.num,
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

impl Rate {
    /// Create a new rate
    ///
    /// # Panics
    ///
    /// This function panics if `num` or `per` is 0.
    pub fn new(num: u64, per: Duration) -> Self {
        assert!(num > 0);
        assert!(per > Duration::from_millis(0));

        Rate { num, per }
    }
}

impl<S, Request> Service<Request> for RateLimit<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = Error<S::Error>;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        match self.state {
            State::Ready { .. } => return Ok(().into()),
            State::Limited(ref mut sleep) => {
                let res = sleep.poll().map_err(|_| Error::RateLimit);

                try_ready!(res);
            }
        }

        self.state = State::Ready {
            until: Instant::now() + self.rate.per,
            rem: self.rate.num,
        };

        Ok(().into())
    }

    fn call(&mut self, request: Request) -> Self::Future {
        match self.state {
            State::Ready { mut until, mut rem } => {
                let now = Instant::now();

                // If the period has elapsed, reset it.
                if now >= until {
                    until = now + self.rate.per;
                    let rem = self.rate.num;

                    self.state = State::Ready { until, rem }
                }

                if rem > 1 {
                    rem -= 1;
                    self.state = State::Ready { until, rem };
                } else {
                    // The service is disabled until further notice
                    let sleep = Delay::new(until);
                    self.state = State::Limited(sleep);
                }

                // Call the inner future
                let inner = self.inner.call(request);
                ResponseFuture { inner }
            }
            State::Limited(..) => panic!("service not ready; poll_ready must be called first"),
        }
    }
}

impl<T> Future for ResponseFuture<T>
where
    T: Future,
{
    type Item = T::Item;
    type Error = Error<T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll().map_err(Error::Upstream)
    }
}

// ===== impl Error =====

impl<T> fmt::Display for Error<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Upstream(ref why) => fmt::Display::fmt(why, f),
            Error::RateLimit => f.pad("rate limit exceeded"),
        }
    }
}

impl<T> error::Error for Error<T>
where
    T: error::Error,
{
    fn cause(&self) -> Option<&error::Error> {
        if let Error::Upstream(ref why) = *self {
            Some(why)
        } else {
            None
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Upstream(_) => "upstream service error",
            Error::RateLimit => "rate limit exceeded",
        }
    }
}
