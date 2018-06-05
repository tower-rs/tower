//! Exercises load balancers with mocked services.

extern crate env_logger;
#[macro_use]
extern crate futures;
extern crate hdrsample;
#[macro_use]
extern crate log;
extern crate rand;
extern crate tokio;
extern crate tower_balance;
extern crate tower_buffer;
extern crate tower_discover;
extern crate tower_in_flight_limit;
extern crate tower_service;

use futures::{future, stream, Async, Future, Poll, Stream};
use hdrsample::Histogram;
use rand::Rng;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::{runtime, timer};
use tower_balance as lb;
use tower_buffer::Buffer;
use tower_discover::{Change, Discover};
use tower_in_flight_limit::InFlightLimit;
use tower_service::Service;

const REQUESTS: usize = 50_000;
const CONCURRENCY: usize = 50;
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
    let executor = rt.executor();

    let exec = executor.clone();
    let fut = future::lazy(move || {
        let decay = Duration::from_secs(10);
        let d = gen_disco(exec.clone());
        let pe = lb::Balance::p2c(lb::load::WithPeakEwma::new(d, decay));
        run("P2C+PeakEWMA", pe, &exec)
    });

    let exec = executor.clone();
    let fut = fut.and_then(move |_| {
        let d = gen_disco(exec.clone());
        let ll = lb::Balance::p2c(lb::load::WithPendingRequests::new(d));
        run("P2C+LeastLoaded", ll, &exec)
    });

    let exec = executor;
    let fut = fut.and_then(move |_| {
        let rr = lb::Balance::round_robin(gen_disco(exec.clone()));
        run("RoundRobin", rr, &exec)
    });

    rt.spawn(fut);
    rt.shutdown_on_idle().wait().unwrap();
}

fn gen_disco(executor: runtime::TaskExecutor) -> Disco {
    use self::Change::Insert;

    let mut changes = VecDeque::new();
    for (i, latency) in MAX_ENDPOINT_LATENCIES.iter().enumerate() {
        changes.push_back(Insert(i, DelayService(*latency)));
    }

    Disco { changes, executor }
}

fn run<D, C>(
    name: &'static str,
    lb: lb::Balance<D, C>,
    executor: &runtime::TaskExecutor,
) -> impl Future<Item = (), Error = ()>
where
    D: Discover<Request = Req, Response = Rsp> + Send + 'static,
    D::Key: Send,
    D::Service: Send,
    D::Error: Send,
    D::DiscoverError: Send,
    <D::Service as Service>::Future: Send,
    C: lb::Choose<D::Key, D::Service> + Send + 'static,
{
    println!("{}", name);
    let t0 = Instant::now();
    compute_histo(SendRequests::new(lb, REQUESTS, CONCURRENCY, executor))
        .map(move |h| report(&h, t0.elapsed()))
        .map_err(|_| {})
}

fn compute_histo<S>(times: S) -> impl Future<Item = Histogram<u64>, Error = S::Error> + 'static
where
    S: Stream<Item = Rsp> + 'static,
{
    // The max delay is 2000ms. At 3 significant figures.
    let histo = Histogram::<u64>::new_with_max(3_000, 3).unwrap();
    times.fold(histo, |mut histo, Rsp { latency }| {
        let ms = latency.as_secs() * 1_000;
        let ms = ms + u64::from(latency.subsec_nanos()) / 1_000 / 1_000;
        histo += ms;
        future::ok(histo)
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

#[derive(Debug)]
struct DelayService(Duration);

#[derive(Debug)]
struct Delay {
    delay: timer::Delay,
    start: Instant,
}

struct Disco {
    changes: VecDeque<Change<usize, DelayService>>,
    executor: runtime::TaskExecutor,
}

#[derive(Debug)]
struct Req;

#[derive(Debug)]
struct Rsp {
    latency: Duration,
}

impl Service for DelayService {
    type Request = Req;
    type Response = Rsp;
    type Error = timer::Error;
    type Future = Delay;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        debug!("polling delay service: ready");
        Ok(Async::Ready(()))
    }

    fn call(&mut self, _: Req) -> Delay {
        let start = Instant::now();
        let maxms = u64::from(self.0.subsec_nanos() / 1_000 / 1_000)
            .saturating_add(self.0.as_secs().saturating_mul(1_000));
        let latency = Duration::from_millis(rand::thread_rng().gen_range(0, maxms));
        Delay {
            delay: timer::Delay::new(start + latency),
            start,
        }
    }
}

impl Future for Delay {
    type Item = Rsp;
    type Error = timer::Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        try_ready!(self.delay.poll());
        let rsp = Rsp {
            latency: Instant::now() - self.start,
        };
        Ok(Async::Ready(rsp))
    }
}

impl Discover for Disco {
    type Key = usize;
    type Request = Req;
    type Response = Rsp;
    type Error = tower_in_flight_limit::Error<tower_buffer::Error<timer::Error>>;
    type Service = InFlightLimit<Buffer<DelayService>>;
    type DiscoverError = ();

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::DiscoverError> {
        match self.changes.pop_front() {
            Some(Change::Insert(k, svc)) => {
                let svc = Buffer::new(svc, &self.executor).unwrap();
                let svc = InFlightLimit::new(svc, ENDPOINT_CAPACITY);
                Ok(Async::Ready(Change::Insert(k, svc)))
            }
            Some(Change::Remove(k)) => Ok(Async::Ready(Change::Remove(k))),
            None => Ok(Async::NotReady),
        }
    }
}

struct SendRequests<D, C>
where
    D: Discover<Request = Req, Response = Rsp>,
    C: lb::Choose<D::Key, D::Service>,
{
    send_remaining: usize,
    lb: InFlightLimit<Buffer<lb::Balance<D, C>>>,
    responses: stream::FuturesUnordered<
        tower_in_flight_limit::ResponseFuture<tower_buffer::ResponseFuture<lb::Balance<D, C>>>,
    >,
}

impl<D, C> SendRequests<D, C>
where
    D: Discover<Request = Req, Response = Rsp> + Send + 'static,
    D::Key: Send,
    D::Service: Send,
    D::Error: Send,
    D::DiscoverError: Send,
    <D::Service as Service>::Future: Send,
    C: lb::Choose<D::Key, D::Service> + Send + 'static,
{
    pub fn new(
        lb: lb::Balance<D, C>,
        total: usize,
        concurrency: usize,
        executor: &runtime::TaskExecutor,
    ) -> Self {
        Self {
            send_remaining: total,
            lb: InFlightLimit::new(Buffer::new(lb, executor).ok().expect("buffer"), concurrency),
            responses: stream::FuturesUnordered::new(),
        }
    }
}

impl<D, C> Stream for SendRequests<D, C>
where
    D: Discover<Request = Req, Response = Rsp>,
    C: lb::Choose<D::Key, D::Service>,
{
    type Item = Rsp;
    type Error =
        tower_in_flight_limit::Error<tower_buffer::Error<<lb::Balance<D, C> as Service>::Error>>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        debug!(
            "sending requests {} / {}",
            self.send_remaining,
            self.responses.len()
        );
        while self.send_remaining > 0 {
            if !self.responses.is_empty() {
                if let Async::Ready(Some(rsp)) = self.responses.poll()? {
                    return Ok(Async::Ready(Some(rsp)));
                }
            }

            debug!("polling lb ready");
            try_ready!(self.lb.poll_ready());

            debug!("sending request");
            let rsp = self.lb.call(Req);
            self.responses.push(rsp);

            self.send_remaining -= 1;
        }

        if !self.responses.is_empty() {
            return self.responses.poll();
        }

        Ok(Async::Ready(None))
    }
}
