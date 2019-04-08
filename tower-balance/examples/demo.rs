//! Exercises load balancers with mocked services.

use env_logger;
use futures::{future, stream, Future, Stream};
use hdrsample::Histogram;
use rand::{self, Rng};
use std::time::{Duration, Instant};
use tokio::{runtime, timer};
use tower::{
    discover::Discover, limit::concurrency::ConcurrencyLimit, util::ServiceFn, Service, ServiceExt,
};
use tower_balance as lb;

const REQUESTS: usize = 50_000;
const CONCURRENCY: usize = 500;
const DEFAULT_RTT: Duration = Duration::from_millis(30);
static ENDPOINT_CAPACITY: usize = CONCURRENCY;
static MAX_ENDPOINT_LATENCIES: [Duration; 10] = [
    Duration::from_millis(1),
    Duration::from_millis(5),
    Duration::from_millis(10),
    Duration::from_millis(10),
    Duration::from_millis(10),
    Duration::from_millis(100),
    Duration::from_millis(100),
    Duration::from_millis(100),
    Duration::from_millis(500),
    Duration::from_millis(1000),
];
static WEIGHTS: [f64; 10] = [1.0, 1.0, 1.0, 0.5, 1.5, 0.5, 1.5, 1.0, 1.0, 1.0];

struct Summary {
    latencies: Histogram<u64>,
    start: Instant,
    count_by_instance: [usize; 10],
}

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
    print!("WEIGHTS=[");
    for w in &WEIGHTS {
        print!("{}, ", w);
    }
    println!("]");

    let mut rt = runtime::Runtime::new().unwrap();

    // Show weighted behavior first...

    let fut = future::lazy(move || {
        let decay = Duration::from_secs(10);
        let d = gen_disco();
        let pe = lb::Balance::p2c(lb::WithWeighted::from(lb::load::WithPeakEwma::new(
            d,
            DEFAULT_RTT,
            decay,
            lb::load::NoInstrument,
        )));
        run("P2C+PeakEWMA w/ weights", pe)
    });

    let fut = fut.then(move |_| {
        let d = gen_disco();
        let ll = lb::Balance::p2c(lb::WithWeighted::from(lb::load::WithPendingRequests::new(
            d,
            lb::load::NoInstrument,
        )));
        run("P2C+LeastLoaded w/ weights", ll)
    });

    // Then run through standard comparisons...

    let fut = fut.then(move |_| {
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

    let fut = fut.then(move |_| {
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

type Error = Box<dyn std::error::Error + Send + Sync>;

fn gen_disco() -> impl Discover<
    Key = usize,
    Error = impl Into<Error>,
    Service = lb::Weighted<
        impl Service<Req, Response = Rsp, Error = Error, Future = impl Send> + Send,
    >,
> + Send {
    let svcs = MAX_ENDPOINT_LATENCIES
        .iter()
        .zip(WEIGHTS.iter())
        .enumerate()
        .map(|(instance, (latency, weight))| {
            let svc = ServiceFn::new(move |_| {
                let start = Instant::now();

                let maxms = u64::from(latency.subsec_nanos() / 1_000 / 1_000)
                    .saturating_add(latency.as_secs().saturating_mul(1_000));
                let latency = Duration::from_millis(rand::thread_rng().gen_range(0, maxms));

                timer::Delay::new(start + latency).map(move |_| {
                    let latency = start.elapsed();
                    Rsp { latency, instance }
                })
            });

            let svc = ConcurrencyLimit::new(svc, ENDPOINT_CAPACITY);
            lb::Weighted::new(svc, *weight)
        });
    tower_discover::ServiceList::new(svcs)
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

    let requests = stream::repeat::<_, Error>(Req).take(REQUESTS as u64);
    let service = ConcurrencyLimit::new(lb, CONCURRENCY);
    let responses = service.call_all(requests).unordered();

    compute_histo(responses).map(|s| s.report()).map_err(|_| {})
}

fn compute_histo<S>(times: S) -> impl Future<Item = Summary, Error = Error> + 'static
where
    S: Stream<Item = Rsp, Error = Error> + 'static,
{
    times.fold(Summary::new(), |mut summary, rsp| {
        summary.count(rsp);
        Ok(summary) as Result<_, Error>
    })
}

impl Summary {
    fn new() -> Self {
        Self {
            // The max delay is 2000ms. At 3 significant figures.
            latencies: Histogram::<u64>::new_with_max(3_000, 3).unwrap(),
            start: Instant::now(),
            count_by_instance: [0; 10],
        }
    }

    fn count(&mut self, rsp: Rsp) {
        let ms = rsp.latency.as_secs() * 1_000;
        let ms = ms + u64::from(rsp.latency.subsec_nanos()) / 1_000 / 1_000;
        self.latencies += ms;
        self.count_by_instance[rsp.instance] += 1;
    }

    fn report(&self) {
        let mut total = 0;
        for c in &self.count_by_instance {
            total += c;
        }
        for (i, c) in self.count_by_instance.into_iter().enumerate() {
            let p = *c as f64 / total as f64 * 100.0;
            println!("  [{:02}] {:>5.01}%", i, p);
        }

        println!("  wall {:4}s", self.start.elapsed().as_secs());

        if self.latencies.len() < 2 {
            return;
        }
        println!("  p50  {:4}ms", self.latencies.value_at_quantile(0.5));

        if self.latencies.len() < 10 {
            return;
        }
        println!("  p90  {:4}ms", self.latencies.value_at_quantile(0.9));

        if self.latencies.len() < 50 {
            return;
        }
        println!("  p95  {:4}ms", self.latencies.value_at_quantile(0.95));

        if self.latencies.len() < 100 {
            return;
        }
        println!("  p99  {:4}ms", self.latencies.value_at_quantile(0.99));

        if self.latencies.len() < 1000 {
            return;
        }
        println!("  p999 {:4}ms", self.latencies.value_at_quantile(0.999));
    }
}

#[derive(Debug, Clone)]
struct Req;

#[derive(Debug)]
struct Rsp {
    latency: Duration,
    instance: usize,
}
