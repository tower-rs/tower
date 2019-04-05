extern crate futures;
extern crate hdrhistogram;
#[macro_use]
extern crate log;
extern crate tokio_timer;
extern crate tower_service;
extern crate tower_filter;

use futures::future::FutureResult;
use futures::future;
use tower_filter::Filter;
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod select;
mod latency;
mod delay;
mod rotating_histogram;

use delay::Delay;
use select::Select;
use latency::Latency;
use rotating_histogram::RotatingHistogram;

type Histo = Arc<Mutex<RotatingHistogram>>;
pub type Service<InnerSvc, P> = select::Select<
    SelectPolicy<P>,
    Latency<Histo, InnerSvc>,
    Delay<DelayPolicy, 
        Filter<
            Latency<Histo, InnerSvc>,
            PolicyPredicate<P>
        >
    >
>;
type Error = Box<dyn std::error::Error + Send + Sync>;

/// A policy which describes which requests can be cloned and then whether those
/// requests should be retried.
pub trait Policy<Request> {
    /// clone_request is called when the request is first received to determine
    /// if the request is retryable.
    fn clone_request(&self, req: &Request) -> Option<Request>;
    /// can_retry is called after the hedge timeout to determine if the hedge
    /// retry should be issued.
    fn can_retry(&self, req: &Request) -> bool;
}

#[derive(Clone)]
pub struct PolicyPredicate<P>(P);
pub struct DelayPolicy {
    histo: Histo,
    latency_percentile: f32,
}
pub struct SelectPolicy<P> {
    policy: P,
    histo: Histo,
    min_data_points: u64,
}

/// A middleware that pre-emptively retries requests which have been outstanding
/// for longer than a given latency percentile.  If either of the original
/// future or the retry future completes, that value is used.
pub fn service<S, P, Request>(
    service: S,
    policy: P,
    min_data_points: u64,
    latency_percentile: f32,
    period: Duration,
) -> Service<S, P>
where
    S: tower_service::Service<Request> + Clone,
    S::Error: Into<Error>,
    P: Policy<Request> + Clone,
{
    let histo = Arc::new(Mutex::new(RotatingHistogram::new(period)));
    service_with_histo(service, policy, min_data_points, latency_percentile, histo)
}

/// A hedge middleware with a prepopulated latency histogram.  This is usedful
/// for integration tests.
pub fn service_with_mock_latencies<S, P, Request>(
    service: S,
    policy: P,
    min_data_points: u64,
    latency_percentile: f32,
    period: Duration,
    latencies_ms: &[u64],
) -> Service<S, P>
where
    S: tower_service::Service<Request> + Clone,
    S::Error: Into<Error>,
    P: Policy<Request> + Clone,
{
    let histo = Arc::new(Mutex::new(RotatingHistogram::new(period)));
    {
        let mut locked = histo.lock().unwrap();
        for latency in latencies_ms.iter() {
            locked.read().record(*latency).unwrap();
        }
    }
    service_with_histo(service, policy, min_data_points, latency_percentile, histo)
}

fn service_with_histo<S, P, Request>(
    service: S,
    policy: P,
    min_data_points: u64,
    latency_percentile: f32,
    histo: Histo,
) -> Service<S, P>
where
    S: tower_service::Service<Request> + Clone,
    S::Error: Into<Error>,
    P: Policy<Request> + Clone,
{
    // Clone the underlying service and wrap both copies in a middleware that
    // records the latencies in a rotating histogram.
    let recorded_a = Latency::new(histo.clone(), service.clone());
    let recorded_b = Latency::new(histo.clone(), service);
    
    // Check policy to see if the hedge request should be issued.
    let filtered = Filter::new(recorded_b, PolicyPredicate(policy.clone()));
    
    // Delay the second request by a percentile of the recorded request latency
    // histogram.
    let delay_policy = DelayPolicy {
        histo: histo.clone(),
        latency_percentile
    };
    let delayed = Delay::new(delay_policy, filtered);
    
    // If the request is retryable, issue two requests -- the second one delayed
    // by a latency percentile.  Use the first result to complete.
    let select_policy = SelectPolicy {
        policy,
        histo,
        min_data_points
    };
    Select::new(select_policy, recorded_a, delayed)
}

// TODO: Remove when Duration::as_millis() becomes stable.
fn duration_as_millis(d: Duration) -> u64 {
    d.as_secs() * 1_000 + d.subsec_millis() as u64
}

impl latency::Record for Histo {
    fn record(&mut self, latency: Duration) {
        let mut locked = self.lock().unwrap();
        locked.write()
            .record(duration_as_millis(latency))
            .unwrap_or_else(|e| {
                error!("Failed to write to hedge histogram: {:?}", e);
            })
    }
}

impl<P, Request> tower_filter::Predicate<Request> for PolicyPredicate<P>
where
    P: Policy<Request>,
{
    type Future = future::Either<FutureResult<(), tower_filter::error::Error>, future::Empty<(), tower_filter::error::Error>>;    //FutureResult<(), tower_filter::error::Error>;

    fn check(&mut self, request: &Request) -> Self::Future {
        if self.0.can_retry(request) {
            future::Either::A(future::ok(()))
        } else {
            // If the hedge retry should not be issued, we simply want to wait
            // for the result of the original request.  Therefore we don't want
            // to return an error here.  Instead, we use future::empty to ensure
            // that the original request wins the select.
            future::Either::B(future::empty())
        }
    }
}

impl<Request> delay::Policy<Request> for DelayPolicy {
    fn delay(&self, _req: &Request) -> Duration {
        let mut locked = self.histo.lock().unwrap();
        let millis = locked.read().value_at_quantile(self.latency_percentile.into());
        Duration::from_millis(millis)
    }
}

impl<P, Request> select::Policy<Request> for SelectPolicy<P>
where
    P: Policy<Request>
{
    fn clone_request(&self, req: &Request) -> Option<Request> {
        self.policy.clone_request(req).filter(|_| {
            let mut locked = self.histo.lock().unwrap();
            // Do not attempt a retry if there are insufficiently many data
            // points in the histogram.
            locked.read().len() >= self.min_data_points
        })
    }
}
