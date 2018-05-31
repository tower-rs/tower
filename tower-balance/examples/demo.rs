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

use futures::{Async, Future, Stream, Poll, future, stream};
use hdrsample::Histogram;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::{runtime, timer};
use tower_balance::*;
use tower_discover::{Change, Discover};
use tower_service::Service;

struct DelayService(Duration);

struct Delay(timer::Delay, Instant);

struct Disco(VecDeque<Change<usize, DelayService>>);

impl Service for DelayService {
    type Request = ();
    type Response = Duration;
    type Error = timer::Error;
    type Future = Delay;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        debug!("polling delay service: ready");
        Ok(Async::Ready(()))
    }

    fn call(&mut self, _: ()) -> Delay {
        let now = Instant::now();
        Delay(timer::Delay::new(now + self.0), now)
    }
}

impl Future for Delay {
    type Item = Duration;
    type Error = timer::Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        try_ready!(self.0.poll());
        Ok(Async::Ready(Instant::now() - self.1))
    }
}

impl Discover for Disco {
    type Key = usize;
    type Request = ();
    type Response = Duration;
    type Error = timer::Error;
    type Service = DelayService;
    type DiscoverError = ();

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::DiscoverError> {
        let r = self.0
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
        changes.push_back(Insert(i, DelayService(quick)));
    }

    let slow = Duration::from_secs(2);
    changes.push_back(Insert(9, DelayService(slow)));

    Disco(changes)
}

struct SendRequests<D, C>
where
    D: Discover<Request = (), Response = Duration, Error = timer::Error>,
    C: Choose<D::Key, D::Service>,
{
    lb: Balance<D, C>,
    send_remaining: usize,
    concurrency: usize,
    responses: stream::FuturesUnordered<ResponseFuture<<D::Service as Service>::Future, D::DiscoverError>>,
}

impl<D, C> SendRequests<D, C>
where
    D: Discover<Request = (), Response = Duration, Error = timer::Error>,
    C: Choose<D::Key, D::Service>,
{
    pub fn new(lb: Balance<D, C>, total: usize, concurrency: usize) -> Self {
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
    D: Discover<Request = (), Response = Duration, Error = timer::Error>,
    C: Choose<D::Key, D::Service>,
{
    type Item = Duration;
    type Error = Error<D::Error, D::DiscoverError>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        debug!("sending requests {} / {}", self.send_remaining, self.responses.len());
        while self.send_remaining > 0 {
            if !self.responses.is_empty() {
                if let Async::Ready(Some(rsp)) = self.responses.poll()? {
                    return Ok(Async::Ready(Some(rsp)));
                }
                if self.responses.len() == self.concurrency {
                    return Ok(Async::NotReady)
                }
            }

            debug!("polling lb ready");
            try_ready!(self.lb.poll_ready());

            debug!("sending request");
            let rsp = self.lb.call(());
            self.responses.push(rsp);

            self.send_remaining -= 1;
        }

        if !self.responses.is_empty() {
            return self.responses.poll();
        }

        Ok(Async::Ready(None))
    }
}

fn compute_histo<S>(times: S)
    -> impl Future<Item = Histogram<u64>, Error = S::Error> + 'static
where
    S: Stream<Item = Duration> + 'static
{
    // The max delay is 2000ms. At 3 significant figures.
    let histo = Histogram::<u64>::new_with_max(3_000, 3).unwrap();
    times.fold(histo, |mut histo, d| {
        let ms = u64::from(d.subsec_nanos()) / 1_000 / 1_000
            + d.as_secs() * 1_000;

        histo += ms;
        future::ok(histo)
    })
}

fn report(pfx: &str, histo: &Histogram<u64>) {
    println!("{} samples: {}", pfx, histo.len());

    if histo.len () < 2 {
        return;
    }
    println!("{} p50:  {}", pfx, histo.value_at_quantile(0.5));

    if histo.len () < 10 {
        return;
    }
    println!("{} p90:  {}", pfx, histo.value_at_quantile(0.9));

    if histo.len () < 50 {
        return;
    }
    println!("{} p95:  {}", pfx, histo.value_at_quantile(0.95));

    if histo.len () < 100 {
        return;
    }
    println!("{} p99:  {}", pfx, histo.value_at_quantile(0.99));

    if histo.len () < 1000 {
        return;
    }
    println!("{} p999: {}", pfx, histo.value_at_quantile(0.999));
}

fn main() {
    env_logger::init();

    let requests = 1_000_000;

    {
        let lb = {
            let loaded = load::WithPendingRequests::new(gen_disco());
            power_of_two_choices(loaded)
        };
        let fut = compute_histo(SendRequests::new(lb, requests, 10_000))
            .map(|histo| report("p2c+ll", &histo))
            .map_err(|_| {});
        runtime::run(fut);
    }

    {
        let lb = {
            let decay = Duration::from_secs(5);
            power_of_two_choices(load::WithPeakEWMA::<_, load::NoMeasure>::new(gen_disco(), decay))
        };
        let fut = compute_histo(SendRequests::new(lb, requests, 10_000))
            .map(|histo| report("p2c+pe", &histo))
            .map_err(|_| {});
        runtime::run(fut);
    }

    {
        let lb = round_robin(gen_disco());
        let fut = compute_histo(SendRequests::new(lb, requests, 10_000))
            .map(|histo| report("rr", &histo))
            .map_err(|_| {});
        runtime::run(fut);
    }
}
