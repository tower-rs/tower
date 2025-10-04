use super::Rate;
use std::{
    future::Future,
    ops::DerefMut,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, ready},
};
use tokio::time::{Instant, Sleep};
use tower_service::Service;

/// Enforces a rate limit on the number of requests the underlying
/// service can handle over a period of time.
#[derive(Debug)]
struct RateLimitInner<T> {
    inner: T,
    rate: Rate,
    state: State,
    sleep: Pin<Box<Sleep>>,
}

#[derive(Debug)]
enum State {
    // The service has hit its limit
    Limited,
    Ready { until: Instant, rem: u64 },
}

impl<T> RateLimitInner<T> {
    /// Create a new rate limiter
    pub fn new(inner: T, rate: Rate) -> Self {
        let until = Instant::now();
        let state = State::Ready {
            until,
            rem: rate.num(),
        };

        RateLimitInner {
            inner,
            rate,
            state,
            // The sleep won't actually be used with this duration, but
            // we create it eagerly so that we can reset it in place rather than
            // `Box::pin`ning a new `Sleep` every time we need one.
            sleep: Box::pin(tokio::time::sleep_until(until)),
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

impl<S, Request> Service<Request> for RateLimitInner<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.state {
            State::Ready { .. } => return Poll::Ready(ready!(self.inner.poll_ready(cx))),
            State::Limited => {
                if Pin::new(&mut self.sleep).poll(cx).is_pending() {
                    tracing::trace!("rate limit exceeded; sleeping.");
                    return Poll::Pending;
                }
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
                    rem = self.rate.num();
                }

                if rem > 1 {
                    rem -= 1;
                    self.state = State::Ready { until, rem };
                } else {
                    // The service is disabled until further notice
                    // Reset the sleep future in place, so that we don't have to
                    // deallocate the existing box and allocate a new one.
                    self.sleep.as_mut().reset(until);
                    self.state = State::Limited;
                }

                // Call the inner future
                self.inner.call(request)
            }
            State::Limited => panic!("service not ready; poll_ready must be called first"),
        }
    }
}

#[cfg(feature = "load")]
impl<S> crate::load::Load for RateLimitInner<S>
where
    S: crate::load::Load,
{
    type Metric = S::Metric;
    fn load(&self) -> Self::Metric {
        self.inner.load()
    }
}

#[derive(Clone, Debug)]
pub struct RateLimit<T> {
    inner: Arc<Mutex<RateLimitInner<T>>>,
}

impl<T> RateLimit<T> {
    /// Create a new rate limiter
    pub fn new(inner: T, rate: Rate) -> Self {
        RateLimit {
            inner: Arc::new(Mutex::new(RateLimitInner::<T>::new(inner, rate))),
        }
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
        self.inner.lock().unwrap().deref_mut().poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.lock().unwrap().deref_mut().call(request)
    }
}
