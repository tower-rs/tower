//! Exercises load balancers with mocked services.

extern crate env_logger;
extern crate futures;
extern crate hdrsample;
extern crate log;
extern crate rand;
extern crate tokio;
extern crate tower;
extern crate tower_balance;
extern crate tower_buffer;
extern crate tower_discover;
extern crate tower_in_flight_limit;
extern crate tower_service;
extern crate tower_service_util;

use futures::{future, stream, Future, Stream};
use hdrsample::Histogram;
use rand::Rng;
use std::time::{Duration, Instant};
use tokio::{runtime, timer};
use tower_balance as lb;
use tower_discover::Discover;
use tower_in_flight_limit::InFlightLimit;
use tower_service::Service;
use tower_service_util::ServiceFn;
use tower::ServiceExt;

const REQUESTS: usize = 50_000;
const CONCURRENCY: usize = 50;
const DEFAULT_RTT: Duration = Duration::from_millis(30);
static ENDPOINT_CAPACITY: usize = CONCURRENCY;
static MAX_ENDPOINT_LATENCIES: [Duration; 10] = [
    Duration::from_millis(1),
    Duration::from_millis(10),
    Duration::from_millis(10),
    Duration::from_millis(10),
    Duration::from_millis(10),
    Duration::from_millis(100),
    Duration::from_millis(100),
    Duration::from_millis(100),
    Duration::from_millis(100),
    Duration::from_millis(1000),
];

fn main() {
    env_logger::init();

    println!("REQUESTS={}", REQUESTS);
    println!("CONCURRENCY={}", CONCURRENCY);
    println!("ENDPOINT_CAPACITY={}", ENDPOINT_CAPACITY);
    print!("MAX_ENDPOINT_LATENCIES=[");
    for max in &MAX_ENDPOINT_LATENCIES {
        let l = max.as_secs() * 1_000 + u64::from(max.subsec_nanos() / 1_000 / 1_000);
        print!("{}ms, ", l);
    }
    println!("]");

    let mut rt = runtime::Runtime::new().unwrap();

    let fut = future::lazy(move || {
        let decay = Duration::from_secs(10);
        let d = gen_disco();
        let pe = lb::Balance::p2c(lb::load::WithPeakEwma::new(
            d,
            DEFAULT_RTT,
            decay,
            lb::load::NoInstrument,
        ));
        run("P2C+PeakEWMA", pe)
    });

    let fut = fut.and_then(move |_| {
        let d = gen_disco();
        let ll = lb::Balance::p2c(lb::load::WithPendingRequests::new(
            d,
            lb::load::NoInstrument,
        ));
        run("P2C+LeastLoaded", ll)
    });

    let fut = fut.and_then(move |_| {
        let rr = lb::Balance::round_robin(gen_disco());
        run("RoundRobin", rr)
    });

    rt.spawn(fut);
    rt.shutdown_on_idle().wait().unwrap();
}

type Error = Box<::std::error::Error + Send + Sync>;

fn gen_disco() -> impl Discover<
    Key = usize,
    Error = impl Into<Error>,
    Service = impl Service<
        Req,
        Response = Rsp,
        Error = Error,
        Future = impl Send,
    > + Send,
> + Send {
    tower_discover::ServiceList::new(MAX_ENDPOINT_LATENCIES.iter().map(|latency| {
        let svc = ServiceFn::new(move |_| {
            let start = Instant::now();
            let maxms = u64::from(latency.subsec_nanos() / 1_000 / 1_000)
                .saturating_add(latency.as_secs().saturating_mul(1_000));
            let latency = Duration::from_millis(rand::thread_rng().gen_range(0, maxms));
            let delay = timer::Delay::new(start + latency);

            delay
                .map(move |_| {
                    let latency = Instant::now() - start;
                    Rsp { latency }
                })
        });

        InFlightLimit::new(svc, ENDPOINT_CAPACITY)
    }))
}

fn run<D, C>(name: &'static str, lb: lb::Balance<D, C>) -> impl Future<Item = (), Error = ()>
where
    D: Discover + Send + 'static,
    D::Error: Into<Error>,
    D::Key: Send,
    D::Service: Service<Req, Response = Rsp, Error = Error> + Send,
    <D::Service as Service<Req>>::Future: Send,
    C: lb::Choose<D::Key, D::Service> + Send + 'static,
{
    println!("{}", name);
    let t0 = Instant::now();

    let requests = stream::repeat::<_, Error>(Req).take(REQUESTS as u64);
    let service = InFlightLimit::new(lb, CONCURRENCY);
    let responses = service.call_all(requests).unordered();

    compute_histo(responses)
        .map(move |h| report(&h, t0.elapsed()))
        .map_err(|_| {})
}

fn compute_histo<S>(times: S) -> impl Future<Item = Histogram<u64>, Error = Error> + 'static
where
    S: Stream<Item = Rsp, Error = Error> + 'static,
{
    // The max delay is 2000ms. At 3 significant figures.
    let histo = Histogram::<u64>::new_with_max(3_000, 3).unwrap();

    times.fold(histo, |mut histo, Rsp { latency }| {
        let ms = latency.as_secs() * 1_000;
        let ms = ms + u64::from(latency.subsec_nanos()) / 1_000 / 1_000;
        histo += ms;
        future::ok::<_, Error>(histo)
    })
}

fn report(histo: &Histogram<u64>, elapsed: Duration) {
    println!("  wall {:4}s", elapsed.as_secs());

    if histo.len() < 2 {
        return;
    }
    println!("  p50  {:4}ms", histo.value_at_quantile(0.5));

    if histo.len() < 10 {
        return;
    }
    println!("  p90  {:4}ms", histo.value_at_quantile(0.9));

    if histo.len() < 50 {
        return;
    }
    println!("  p95  {:4}ms", histo.value_at_quantile(0.95));

    if histo.len() < 100 {
        return;
    }
    println!("  p99  {:4}ms", histo.value_at_quantile(0.99));

    if histo.len() < 1000 {
        return;
    }
    println!("  p999 {:4}ms", histo.value_at_quantile(0.999));
}

#[derive(Debug, Clone)]
struct Req;

#[derive(Debug)]
struct Rsp {
    latency: Duration,
}
