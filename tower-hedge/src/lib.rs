#[macro_use]
extern crate futures;
extern crate hdrhistogram;
#[macro_use]
extern crate log;
extern crate tokio_timer;
extern crate tower_service;

use futures::{Async, Future, Poll};
use hdrhistogram::Histogram;
use tokio_timer::{clock, Delay};
use tower_service::Service;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod rotating;

use rotating::Rotating;

/// A "retry policy" to classify if a request should be pre-emptively retried.
pub trait Policy<Request>: Sized {
    /// Check the policy if a certain request should be pre-emptively retried.
    ///
    /// This method is passed a reference to the original request.
    fn can_retry(&self, req: &Request) -> bool;
    /// Tries to clone a request before being passed to the inner service.
    ///
    /// If the request cannot be cloned, return `None`.
    fn clone_request(&self, req: &Request) -> Option<Request>;
}

/// A middleware that pre-emptively retries requests which have been outstanding
/// for longer than a given latency percentile.  If either of the original
/// future or the retry future completes, that value is used.
#[derive(Clone)]
pub struct Hedge<P, S> {
    policy: P,
    service: S,
    latency_percentile: f32,
    /// Only retry if there are at least this many data points in the histogram.
    min_data_points: u64,
    /// A rotating histogram is used to track response latency.
    pub latency_histogram: Arc<Mutex<Rotating<Histogram<u64>>>>,
}

/// The Future returned by the Hedge service.
pub struct ResponseFuture<P, S, Request>
where
    P: Policy<Request>,
    S: Service<Request>,
{
    /// If the request was clonable, a clone is stored.
    request: Option<Request>,
    /// The time of the original call to the inner service.  Used to calculate
    /// response latency.
    start: Instant,
    hedge: Hedge<P, S>,
    orig_fut: S::Future,
    hedge_fut: Option<S::Future>,
    /// A future representing when to start the hedge request.
    delay: Option<Delay>,
}

// ===== impl Hedge =====

impl<P, S> Hedge<P, S> {
    pub fn new<Request>(
        policy: P,
        service: S,
        latency_percentile: f32,
        rotation_period: Duration,
        min_data_points: u64,
    ) -> Self
    where
        P: Policy<Request> + Clone,
        S: Service<Request>,
    {
        let new: fn() -> Histogram<u64> = || {
            Histogram::<u64>::new_with_bounds(1, 10_000, 3).expect("Invalid histogram params")
        };
        let latency_histogram = Arc::new(Mutex::new(Rotating::new(rotation_period, new)));
        Hedge {
            policy,
            service,
            latency_percentile,
            min_data_points,
            latency_histogram,
        }
    }

    /// Record the latency of a completed request in the latency histogram.
    fn record(&self, start: Instant) {
        let duration = clock::now() - start;
        let mut locked = self.latency_histogram.lock().unwrap();
        locked.write().record(Self::as_millis(duration)).unwrap_or_else(|e| {
            error!("Failed to write to hedge histogram: {:?}", e);
        });
    }

    // TODO: Remove when Duration::as_millis() becomes stable.
    fn as_millis(d: Duration) -> u64 {
        d.as_secs() * 1_000 + d.subsec_millis() as u64
    }
}

impl<P, S, Request> Service<Request> for Hedge<P, S>
where
    P: Policy<Request> + Clone,
    S: Service<Request> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<P, S, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let cloned = self.policy.clone_request(&request);
        let orig_fut = self.service.call(request);

        let start = clock::now();
        // Find the nth percentile latency from the read side of the histogram.
        // Requests which take longer than this will be pre-emptively retried.
        let mut histo = self.latency_histogram.lock().unwrap();
        // TODO: Consider adding a minimum delay for hedge requests (perhaps as
        // a factor of the p50 latency).

        // We will only issue a hedge request if there are sufficiently many
        // data points in the histogram to give us confidence about the
        // distribution.
        let read = histo.read();
        let delay = if read.len() < self.min_data_points {
            trace!("Not enough data points to determine hedge timeout.  Have {}, need {}", 
                read.len(),
                self.min_data_points
            );
            None
        } else {
            let hedge_timeout = read.value_at_quantile(self.latency_percentile.into());
            trace!("Issuing request with hedge timeout of {}ms", hedge_timeout);
            Some(Delay::new(start + Duration::from_millis(hedge_timeout)))
        };

        ResponseFuture {
            request: cloned,
            start,
            hedge: self.clone(),
            orig_fut,
            hedge_fut: None,
            delay,
        }
    }
}

// ===== impl ResponseFuture =====

impl<P, S, Request> Future for ResponseFuture<P, S, Request>
where
    P: Policy<Request> + Clone,
    S: Service<Request> + Clone,
{
    type Item = S::Response;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            // If the original future is complete, return its result.
            match self.orig_fut.poll() {
                Ok(Async::Ready(rsp)) => {
                    self.hedge.record(self.start);
                    return Ok(Async::Ready(rsp));
                }
                Ok(Async::NotReady) => {}
                Err(e) => {
                    self.hedge.record(self.start);;
                    return Err(e);
                }
            }

            if let Some(ref mut hedge_fut) = self.hedge_fut {
                // If the hedge future exists, return its result.
                let p = hedge_fut.poll();
                if let Ok(ref a) = p {
                    if a.is_ready() {
                        trace!("Hedge request complete after {:?}", clock::now() - self.start);
                        self.hedge.record(self.start);
                    }
                }
                return p;
            }
            // Original future is pending, but hedge hasn't started.  Check
            // the delay.
            let delay = match self.delay.as_mut() {
                Some(d) => d,
                // No delay, can't retry.
                None => return Ok(Async::NotReady),
            };
            match delay.poll() {
                Ok(Async::Ready(_)) => {
                    try_ready!(self.hedge.poll_ready());
                    if let Some(req) = self.request.take() {
                        if self.hedge.policy.can_retry(&req) {
                            // Start the hedge request.
                            trace!("Issuing hedge request after {:?}", clock::now() - self.start);
                            self.request = self.hedge.policy.clone_request(&req);
                            self.hedge_fut = Some(self.hedge.service.call(req));
                        } else {
                            // Policy says we can't retry.
                            // Put the taken request back.
                            trace!("Hedge timeout reached, but unable to retry due to policy");
                            self.request = Some(req);
                            return Ok(Async::NotReady);
                        }
                    } else {
                        // No cloned request, can't retry.
                        trace!("Hedge timeout reached, but unable to retry because request is not clonable");
                        return Ok(Async::NotReady);
                    }
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady), // Not time to retry yet.
                Err(e) => {
                    // Timer error, don't retry.
                    error!("Timer error: {:?}", e);
                    return Ok(Async::NotReady);
                },
            }
        }
    }
}

// ===== impl Histogram =====

impl rotating::Clear for Histogram<u64> {
    fn clear(&mut self) {
        Histogram::clear(self);
    }
}

impl rotating::Size for Histogram<u64> {
    fn size(&self) -> u64 {
        self.len()
    }
}
