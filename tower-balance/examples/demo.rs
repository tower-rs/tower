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
extern crate tower_discover;
extern crate tower_service;
extern crate typemap;

use futures::{future, stream, Async, Future, Poll, Stream};
use hdrsample::Histogram;
use rand::Rng;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::{runtime, timer};
use tower_discover::{Change, Discover};
use tower_service::Service;

use tower_balance as lb;

const TOTAL_REQUESTS: usize = 1_000_000;
const CONCURRENCY: usize = 10_000;

fn main() {
    env_logger::init();

    let peak_ewma = {
        let decay = Duration::from_secs(10);
        lb::load::WithPeakEWMA::new(gen_disco(), decay).measured::<RspMeasure>()
    };
    run("p2c+pe", lb::power_of_two_choices(peak_ewma));

    let ll = lb::load::WithPendingRequests::new(gen_disco()).measured::<RspMeasure>();
    run("p2c+ll", lb::power_of_two_choices(ll));

    let rr = lb::round_robin(gen_disco());
    run("rr", rr);
}

struct DelayService(Duration);

struct Delay {
    delay: timer::Delay,
    start: Instant,
}

struct Disco(VecDeque<Change<usize, DelayService>>);

struct Req;

struct RspMeasure;
impl<I: Send + Sync + 'static> lb::load::Measure<I, Rsp> for RspMeasure {
    type Measured = Rsp;
    fn measure(instrument: I, mut rsp: Rsp) -> Rsp {
        rsp.meta.insert::<Instrument<I>>(instrument);
        rsp
    }
}

struct Instrument<I>(I);
impl<I: Send + Sync + 'static> typemap::Key for Instrument<I> {
    type Value = I;
}

struct Rsp {
    latency: Duration,
    meta: typemap::ShareMap,
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
        let maxms = u64::from(self.0.subsec_nanos()) / 1_000 / 1_000 + self.0.as_secs() * 1_000;
        let delay = Duration::from_millis(rand::thread_rng().gen_range(100, maxms));
        Delay {
            delay: timer::Delay::new(start + delay),
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
            meta: typemap::ShareMap::custom(),
            latency: Instant::now() - self.start,
        };
        Ok(Async::Ready(rsp))
    }
}

impl Discover for Disco {
    type Key = usize;
    type Request = Req;
    type Response = Rsp;
    type Error = timer::Error;
    type Service = DelayService;
    type DiscoverError = ();

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::DiscoverError> {
        let r = self
            .0
            .pop_front()
            .map(Async::Ready)
            .unwrap_or(Async::NotReady);
        debug!("polling disco: {:?}", r.is_ready());
        Ok(r)
    }
}

fn gen_disco() -> Disco {
    use self::Change::Insert;

    let mut changes = VecDeque::new();

    let quick = Duration::from_millis(500);
    for i in 0..8 {
        changes.push_back(Insert(i as usize, DelayService(quick)));
    }

    let slow = Duration::from_secs(2);
    changes.push_back(Insert(9, DelayService(slow)));

    Disco(changes)
}

struct SendRequests<D, C>
where
    D: Discover<Request = Req, Response = Rsp, Error = timer::Error>,
    C: lb::Choose<D::Key, D::Service>,
{
    lb: lb::Balance<D, C>,
    send_remaining: usize,
    concurrency: usize,
    responses:
        stream::FuturesUnordered<lb::ResponseFuture<<D::Service as Service>::Future, D::DiscoverError>>,
}

impl<D, C> SendRequests<D, C>
where
    D: Discover<Request = Req, Response = Rsp, Error = timer::Error>,
    C: lb::Choose<D::Key, D::Service>,
{
    pub fn new(lb: lb::Balance<D, C>, total: usize, concurrency: usize) -> Self {
        Self {
            lb,
            send_remaining: total,
            concurrency,
            responses: stream::FuturesUnordered::new(),
        }
    }
}

impl<D, C> Stream for SendRequests<D, C>
where
    D: Discover<Request = Req, Response = Rsp, Error = timer::Error>,
    C: lb::Choose<D::Key, D::Service>,
{
    type Item = Rsp;
    type Error = lb::Error<D::Error, D::DiscoverError>;

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
                if self.responses.len() == self.concurrency {
                    return Ok(Async::NotReady);
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

fn compute_histo<S>(times: S) -> impl Future<Item = Histogram<u64>, Error = S::Error> + 'static
where
    S: Stream<Item = Rsp> + 'static,
{
    // The max delay is 2000ms. At 3 significant figures.
    let histo = Histogram::<u64>::new_with_max(3_000, 3).unwrap();
    times.fold(histo, |mut histo, Rsp { meta, latency }| {
        drop(meta);
        let ms = u64::from(latency.subsec_nanos()) / 1_000 / 1_000 + latency.as_secs() * 1_000;

        histo += ms;
        future::ok(histo)
    })
}

fn report(pfx: &str, histo: &Histogram<u64>) {
    println!("{} samples: {}", pfx, histo.len());

    if histo.len() < 2 {
        return;
    }
    println!("{} p50:  {}", pfx, histo.value_at_quantile(0.5));

    if histo.len() < 10 {
        return;
    }
    println!("{} p90:  {}", pfx, histo.value_at_quantile(0.9));

    if histo.len() < 50 {
        return;
    }
    println!("{} p95:  {}", pfx, histo.value_at_quantile(0.95));

    if histo.len() < 100 {
        return;
    }
    println!("{} p99:  {}", pfx, histo.value_at_quantile(0.99));

    if histo.len() < 1000 {
        return;
    }
    println!("{} p999: {}", pfx, histo.value_at_quantile(0.999));
}

fn run<D, C>(name: &'static str, lb: lb::Balance<D, C>)
where
    D: Discover<Request = Req, Response = Rsp, Error = timer::Error> + Send + 'static,
    D::Key: Send,
    D::Service: Send,
    D::DiscoverError: Send,
    <D::Service as Service>::Future: Send,
    C: lb::Choose<D::Key, D::Service> + Send + 'static,
{
    let f = compute_histo(SendRequests::new(lb, TOTAL_REQUESTS, CONCURRENCY))
        .map(move |h| report(name, &h))
        .map_err(|_| {});
    runtime::run(f);
}
