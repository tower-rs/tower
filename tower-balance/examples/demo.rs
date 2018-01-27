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

#[derive(Clone, Debug)]
struct DelayService(Timer, Duration);

struct Delay(Sleep, Instant);

struct Disco<S>(VecDeque<Change<usize, S>>);

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

impl<S: Service> Discover for Disco<S> {
    type Key = usize;
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Service = S;
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

fn gen_disco(timer: &Timer) -> Disco<DelayService> {
    gen_disco_map(timer, |_, s| s)
}

fn gen_disco_map<F, S>(timer: &Timer, map: F) -> Disco<S>
where
    F: Fn(usize, DelayService) -> S,
    S: Service,
{
    use self::Change::Insert;

    let mut changes = VecDeque::new();

    let quick = DelayService(timer.clone(), Duration::from_millis(200));
    for i in 0..8 {
        let s = map(i, quick.clone());
        changes.push_back(Insert(i, s));
    }

    let slow = map(9, DelayService(timer.clone(), Duration::from_secs(2)));
    changes.push_back(Insert(9, slow));

    Disco(changes)
}

struct SendRequests<D, C>
where
    D: Discover<Request = (), Response = Duration, Error = TimerError>,
    C: Choose<D::Key, D::Service>,
{
    lb: Balance<D, C>,
    send_remaining: usize,
    responses: stream::FuturesUnordered<ResponseFuture<<D::Service as Service>::Future, D::DiscoverError>>,
}

impl<D, C> Stream for SendRequests<D, C>
where
    D: Discover<Request = (), Response = Duration, Error = TimerError>,
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
            }

            if self.send_remaining > 0 {
                debug!("polling lb ready");
                try_ready!(self.lb.poll_ready());

                debug!("sending request");
                let rsp = self.lb.call(());
                self.responses.push(rsp);

                self.send_remaining -= 1;
            }
        }

        if !self.responses.is_empty() {
            return self.responses.poll();
        }

        Ok(Async::Ready(None))
    }
}

fn compute_histo<S>(times: S)
    -> Box<Future<Item = Histogram<u64>, Error = S::Error> + 'static>
where
    S: Stream<Item = Duration> + 'static
{
    // The max delay is 2000ms. At 3 significant figures.
    let histo = Histogram::<u64>::new_with_max(3_000, 3).unwrap();
    let fut = times
        .fold(histo, |mut histo, elapsed| {
            let ns: u32 = elapsed.subsec_nanos();
            let ms = u64::from(ns) / 1_000 / 1_000
                + elapsed.as_secs() * 1_000;
            histo += ms;

            future::ok(histo)
        });

    Box::new(fut)
}

fn report(name: &str, histo: &Histogram<u64>) {
    println!("{}", name);

    if histo.len () < 2 {
        return;
    }
    println!("  p50\t{}", histo.value_at_quantile(0.5));

    if histo.len () < 10 {
        return;
    }
    println!("  p90\t{}", histo.value_at_quantile(0.9));

    if histo.len () < 50 {
        return;
    }
    println!("  p95\t{}", histo.value_at_quantile(0.95));

    if histo.len () < 100 {
        return;
    }
    println!("  p99\t{}", histo.value_at_quantile(0.99));

    if histo.len () < 1000 {
        return;
    }
    println!("  p999\t{}", histo.value_at_quantile(0.999));
}

fn demo<D, C>(name: &str, core: &mut Core, lb: Balance<D, C>)
where
    D: Discover<Request = (), Response = Duration, Error = TimerError> + 'static,
    D::DiscoverError: ::std::fmt::Debug,
    C: Choose<D::Key, D::Service> + 'static,
{
    let send = SendRequests { lb, send_remaining: 1_000_000, responses: stream::FuturesUnordered::new() };
    let histo = core.run(compute_histo(send)).unwrap();
    report(name, &histo);
}

fn main() {
    env_logger::init().unwrap();

    let timer = Timer::default();
    let mut core = Core::new().unwrap();
    let rng =  rand::thread_rng();

    demo("p2c + pending requests", &mut core, {
        let loaded = load::WithPendingRequests::new(gen_disco(&timer));
        power_of_two_choices(loaded, rng.clone())
    });

    demo("p2c + pending requests + weighted", &mut core, {
        // Weight nodes so that all quick nodes are preferred to the slow node.
        let replicas = gen_disco_map(&timer, |i, svc| {
            let w = (10.0 - i as f64).into();
            Weighted::new(load::PendingRequests::new(svc)).weighted(w)
        });
        power_of_two_choices(replicas, rng)
    });

    demo("round robin", &mut core, round_robin(gen_disco(&timer)));
}
