use futures::{Async, Poll};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower_discover::{Change, Discover};
use tower_service::Service;

use super::{Measure, MeasureFuture};

use Load;

pub struct WithPeakEWMA<D, T> {
    discover: D,
    decay: Duration,
    _p: PhantomData<T>,
}

pub struct PeakEWMA<S, T> {
    service: S,
    node: Arc<Mutex<Node>>,
    _p: PhantomData<T>,
}

pub struct Instrument {
    start: Instant,
    node: Arc<Mutex<Node>>,
}

struct Node {
    last_update: Instant,
    cost: f64,
    tau: f64,
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct Cost(f64);

// ===== impl PeakEWMA =====

impl<D, M> WithPeakEWMA<D, M>
where
    D: Discover,
    M: Measure<Instrument, D::Response>,
{
    pub fn new(discover: D, decay: Duration) -> Self {
        Self {
            discover,
            decay,
            _p: PhantomData,
        }
    }
}

impl<D, M> Discover for WithPeakEWMA<D, M>
where
    D: Discover,
    M: Measure<Instrument, D::Response>,
{
    type Key = D::Key;
    type Request = D::Request;
    type Response = M::Measured;
    type Error = D::Error;
    type Service = PeakEWMA<D::Service, M>;
    type DiscoverError = D::DiscoverError;

    /// Yields the next discovery change set.
    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::DiscoverError> {
        use self::Change::*;

        let change = match try_ready!(self.discover.poll()) {
            Insert(k, svc) => Insert(k, PeakEWMA::new(svc, self.decay)),
            Remove(k) => Remove(k),
        };

        Ok(Async::Ready(change))
    }
}

// ===== impl PeakEWMA =====

impl<S, T> PeakEWMA<S, T>
where
    S: Service,
    T: Measure<Instrument, S::Response>,
{
    pub fn new(service: S, decay: Duration) -> Self {
        Self {
            service,
            node: Arc::new(Mutex::new(Node::new(decay))),
            _p: PhantomData,
        }
    }

    fn instrument(&self) -> Instrument {
        Instrument {
            start: Instant::now(),
            node: self.node.clone()
        }
    }
}

impl<S, T> Service for PeakEWMA<S, T>
where
    S: Service,
    T: Measure<Instrument, S::Response>,
{
    type Request = S::Request;
    type Response = T::Measured;
    type Error = S::Error;
    type Future = MeasureFuture<S::Future, T, Instrument>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        MeasureFuture::new(self.instrument(), self.service.call(req))
    }
}

const PENALTY: f64 = 65535.0;

impl<S, T> Load for PeakEWMA<S, T> {
    type Metric = Cost;

    fn load(&self) -> Self::Metric {
        let pending = Arc::strong_count(&self.node) as u32 - 1;
        let mut node = self.node.lock().expect("peak ewma state");
        node.update(0.0);
        if node.cost == 0.0 && pending > 0 {
            Cost(PENALTY + f64::from(pending))
        } else {
            Cost(node.cost * (f64::from(pending) + 1.0))
        }
    }
}

// ===== impl Node =====

impl Node {
    fn new(decay: Duration) -> Self {
        Self {
            tau: nanos(decay),
            last_update: Instant::now(),
            cost: 0.0,
        }
    }

    fn update(&mut self, rtt: f64) -> f64 {
        let now = Instant::now();
        if self.cost < rtt {
            trace!("update rtt={}", rtt);
            self.cost = rtt;
        } else {
            let td = nanos(now - self.last_update);
            let w = (-td / self.tau).exp();
            self.cost = (self.cost * w) + (rtt * (1.0 - w));
            trace!("update rtt={} w={} cost={}", rtt, w, self.cost);
        }
        self.last_update = now;
        self.cost
    }
}

// ===== impl Instrument =====

impl Drop for Instrument {
    fn drop(&mut self) {
        if let Ok(mut node) = self.node.lock() {
            node.update(nanos(self.start.elapsed()));
        }
    }
}

// Utility that converts durations to nanos in f64.
//
// We generally don't care about very large duration values, so it's fine for this to be a
// lossy transformation.
fn nanos(d: Duration) -> f64 {
    let n: f64 = d.subsec_nanos().into();
    let s = (d.as_secs() * 1_000_000_000) as f64;
    n + s
}
