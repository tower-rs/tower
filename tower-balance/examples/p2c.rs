//! Exercises load balancers with mocked services.

extern crate env_logger;
#[macro_use]
extern crate futures;
extern crate hdrsample;
#[macro_use]
extern crate log;
extern crate rand;
extern crate tokio_core;
extern crate tokio_timer;
extern crate tower;
extern crate tower_balance;
extern crate tower_discover;

use futures::{Async, Future, Stream, Poll, future, stream};
use hdrsample::Histogram;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio_core::reactor::Core;
use tokio_timer::{Timer, TimerError, Sleep};
use tower::Service;
use tower_balance::*;
use tower_discover::{Change, Discover};

struct DelayService(Timer, Duration);

struct Delay(Sleep, Instant);

struct Disco(VecDeque<Change<usize, DelayService>>);

impl Service for DelayService {
    type Request = ();
    type Response = Duration;
    type Error = TimerError;
    type Future = Delay;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        debug!("polling delay service: ready");
        Ok(Async::Ready(()))
    }

    fn call(&mut self, _: ()) -> Delay {
        Delay(self.0.sleep(self.1), Instant::now())
    }
}

impl Future for Delay {
    type Item = Duration;
    type Error = TimerError;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        try_ready!(self.0.poll());
        Ok(Async::Ready(self.1.elapsed()))
    }
}

impl Discover for Disco {
    type Key = usize;
    type Request = ();
    type Response = Duration;
    type Error = TimerError;
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

fn gen_disco(timer: &Timer) -> Disco {
    use self::Change::Insert;

    let mut changes = VecDeque::new();

    let quick = Duration::from_millis(100);
    for i in 0..8 {
        changes.push_back(Insert(i, DelayService(timer.clone(), quick)));
    }

    let slow = Duration::from_secs(2);
    changes.push_back((Insert(9, DelayService(timer.clone(), slow))));

    Disco(changes)
}

struct SendRequests<D, C>
where
    D: Discover<Request = (), Response = Duration, Error = TimerError>,
    C: Choose<D::Key, D::Service>,
{
    lb: Balance<D, C>,
    remaining: usize,
    responses: Option<VecDeque<ResponseFuture<<D::Service as Service>::Future, D::DiscoverError>>>,
}

impl<D, C> Future for SendRequests<D, C>
where
    D: Discover<Request = (), Response = Duration, Error = TimerError>,
    C: Choose<D::Key, D::Service>,
{
    type Item = VecDeque<ResponseFuture<<D::Service as Service>::Future, D::DiscoverError>>;
    type Error = Error<D::Error, D::DiscoverError>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        {
            let rsps = self.responses.as_mut().expect("poll after ready");

            while self.remaining > 0 {
                debug!("polling lb ready");
                try_ready!(self.lb.poll_ready());
                debug!("sending request");
                let rsp = self.lb.call(());

                rsps.push_back(rsp);
                self.remaining -= 1;
            }
        }

        let rsps = self.responses.take().expect("poll after ready");
        Ok(Async::Ready(rsps))
    }
}

fn compute_histo<F>(times: VecDeque<F>)
    -> Box<Future<Item = Histogram<u64>, Error = F::Error> + 'static>
where
    F: Future<Item = Duration> + 'static
{
    // The max delay is 2000ms. At 3 significant figures.
    let histo = Histogram::<u64>::new_with_max(3_000, 3).unwrap();
    let fut = stream::futures_unordered::<VecDeque<F>>(times)
        .fold(histo, |mut histo, elapsed| {
            let ns: u32 = elapsed.subsec_nanos();
            let ms = u64::from(ns) / 1_000 / 1_000
                + elapsed.as_secs() * 1_000;
            histo += ms;

            future::ok(histo)
        });

    Box::new(fut)
}

fn main() {
    env_logger::init().unwrap();
    info!("sup");

    let timer = Timer::default();

    let lb = {
        let loaded = load::WithPendingRequests::new(gen_disco(&timer));
        power_of_two_choices(loaded, rand::thread_rng())
    };

    let mut core = Core::new().unwrap();
    let send = SendRequests { lb, remaining: 100_000, responses: Some(VecDeque::with_capacity(100_000)) };
    let histo = core.run(send.and_then(compute_histo)).unwrap();

    println!("samples: {}", histo.len());

    if histo.len () < 2 {
        return;
    }
    println!("p50:  {}", histo.value_at_quantile(0.5));

    if histo.len () < 10 {
        return;
    }
    println!("p90:  {}", histo.value_at_quantile(0.9));

    if histo.len () < 50 {
        return;
    }
    println!("p95:  {}", histo.value_at_quantile(0.95));

    if histo.len () < 100 {
        return;
    }
    println!("p99:  {}", histo.value_at_quantile(0.99));

    if histo.len () < 1000 {
        return;
    }
    println!("p999: {}", histo.value_at_quantile(0.999));
}
