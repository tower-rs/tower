use futures::Poll;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower_service::Service;

use super::{Measure, MeasureFuture};

use Load;

pub struct PeakEWMA<S, T> {
    service: S,
    state: Arc<Mutex<State>>,
    _p: PhantomData<T>,
}

pub struct Instrument {
    start: Instant,
    state: Arc<Mutex<State>>,
}

struct State {
    pending: u32,
    stamp: Instant,
    tau: f64,
    cost: f64,
}

pub struct Cost(f64);

// ===== impl PeakEWMA =====

impl<S, T> PeakEWMA<S, T>
where
    S: Service,
    T: Measure<Instrument, S::Response>,
{
    pub fn new(service: S, decay: Duration) -> Self {
        let state = State {
            pending: 0,
            cost: 0.0,
            tau: nanos(decay),
            stamp: Instant::now(),
        };

        Self {
            service,
            state: Arc::new(Mutex::new(state)),
            _p: PhantomData,
        }
    }

    fn instrument(&self) -> Instrument {
        {
            // TODO replace this with Arc counting
            let mut state = self.state.lock().expect("lock peak ewma state");
            state.pending += 1;
        }
        Instrument {
            start: Instant::now(),
            state: self.state.clone()
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

impl<S, T> Load for PeakEWMA<S, T> {
    type Metric = Cost;

    fn load(&self) -> Self::Metric {
        // TODO apply a penalty when there is no load information
        let mut state = self.state.lock().expect("peak ewma state");
        state.update(0.0);
        let pending: f64 = state.pending.into();
        Cost(state.cost * (pending + 1.0))
    }
}

// ===== impl State =====

impl State {
    fn update(&mut self, rtt: f64) {
        let now = Instant::now();
        if self.cost < rtt {
            self.cost = rtt;
        } else {
            let td = nanos(now - self.stamp);
            let w = (-td / self.tau).exp();
            self.cost = (self.cost * w) + (rtt * (1.0 - w));
        }
        self.stamp = now;
    }
}

// ===== impl Instrument =====

impl Drop for Instrument {
    fn drop(&mut self) {
        if let Ok(mut s) = self.state.lock() {
            s.pending -= 1;
            s.update(nanos(self.start.elapsed()))
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
