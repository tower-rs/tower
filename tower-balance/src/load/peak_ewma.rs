use futures::{Async, Poll};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower_discover::{Change, Discover};
use tower_service::Service;

use super::{Measure, MeasureFuture, NoMeasure};

use Load;

pub struct WithPeakEWMA<D, M> {
    discover: D,
    decay: Duration,
    _p: PhantomData<M>,
}

pub struct PeakEWMA<S, M> {
    service: S,
    node: Arc<Mutex<Node>>,
    _p: PhantomData<M>,
}

pub struct Instrument {
    start: Instant,
    node: Arc<Mutex<Node>>,
}

struct Node {
    last_update: Instant,
    rtt_estimate: f64,
    tau: f64,
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct Cost(f64);

// ===== impl PeakEWMA =====

impl<D: Discover> WithPeakEWMA<D, NoMeasure> {
    pub fn new(discover: D, decay: Duration) -> Self {
        Self {
            discover,
            decay,
            _p: PhantomData,
        }
    }

    pub fn measured<M>(self) -> WithPeakEWMA<D, M>
    where
        M: Measure<Instrument, D::Response>,
    {
        WithPeakEWMA {
            discover: self.discover,
            decay: self.decay,
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

impl<S, M> PeakEWMA<S, M>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
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
            node: self.node.clone(),
        }
    }
}

impl<S, M> Service for PeakEWMA<S, M>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    type Request = S::Request;
    type Response = M::Measured;
    type Error = S::Error;
    type Future = MeasureFuture<S::Future, M, Instrument>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        MeasureFuture::new(self.instrument(), self.service.call(req))
    }
}

const PENALTY: f64 = 65535.0;

impl<S, M> Load for PeakEWMA<S, M> {
    type Metric = Cost;

    fn load(&self) -> Self::Metric {
        let pending = Arc::strong_count(&self.node) as u32 - 1;
        let mut node = self.node.lock().expect("peak ewma state");
        if node.rtt_estimate == 0.0 && pending > 0 {
            debug!("penalty={} pending={}", PENALTY, pending);
            Cost(PENALTY + f64::from(pending))
        } else {
            // Update the cost to account for decay since the last update.
            let rtt_estimate = node.update_rtt_estimate(0.0);

            let cost = rtt_estimate * (f64::from(pending) + 1.0);
            debug!(
                "load estimate={:.0}ms pending={} cost={:.0}",
                node.rtt_estimate / 1_000_000.0,
                pending,
                cost
            );
            Cost(cost)
        }
    }
}

// ===== impl Node =====

impl Node {
    fn new(decay: Duration) -> Self {
        Self {
            tau: nanos(decay),
            last_update: Instant::now(),
            rtt_estimate: 0.0,
        }
    }

    fn update_rtt_estimate(&mut self, rtt: f64) -> f64 {
        let now = Instant::now();
        if self.rtt_estimate < rtt {
            trace!("update peak rtt={}", rtt);
            self.rtt_estimate = rtt;
        } else {
            // Determine how heavily to weight our rtt estimate.
            let td = nanos(now - self.last_update);
            let recency = (-td / self.tau).exp();

            let e = (self.rtt_estimate * recency) + (rtt * (1.0 - recency));
            trace!(
                "update prior={:.0}ms * {:.5} + rtt={:.0}ms * {:.5}; estimate={:.0}ms",
                self.rtt_estimate / 1_000_000.0,
                recency,
                rtt / 1_000_000.0,
                1.0 - recency,
                e / 1_000_000.0
            );
            self.rtt_estimate = e;
        }
        self.last_update = now;
        self.rtt_estimate
    }
}

// ===== impl Instrument =====

impl Drop for Instrument {
    fn drop(&mut self) {
        if let Ok(mut node) = self.node.lock() {
            let rtt = self.start.elapsed();
            trace!("drop:; rtt={:?}", rtt);
            node.update_rtt_estimate(nanos(rtt));
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
