use futures::Poll;
use std::{
    marker::PhantomData, sync::{Arc, Mutex}, time::{Duration, Instant},
};
use tower_service::Service;

use super::track::{Track, TrackFuture};

use Load;

pub struct PeakEWMA<S, T> {
    service: S,
    state: Arc<Mutex<State>>,
    _p: PhantomData<T>,
}

pub struct Tracker(Arc<Mutex<State>>);

struct State {
    pending: usize,
    stamp: Instant,
    decay: Duration,
    cost: f64,
}

pub struct Cost(f64);

// ===== impl PeakEWMA =====

impl<S, T> PeakEWMA<S, T>
where
    S: Service,
    T: Track<Tracker, S::Response>,
{
    pub fn new(service: S, decay: Duration) -> Self {
        let state = State {
            pending: 0,
            cost: 0.0,
            decay,
            stamp: Instant::now(),
        };

        Self {
            service,
            state: Arc::new(Mutex::new(state)),
            _p: PhantomData,
        }
    }

    fn tracker(&self) -> Tracker {
        Tracker(self.state.clone())
    }
}

impl<S, T> Load for PeakEWMA<S, T> {
    type Metric = Cost;

    fn load(&self) -> Self::Metric {
        unimplemented!()
    }
}

impl<S, T> Service for PeakEWMA<S, T>
where
    S: Service,
    T: Track<Tracker, S::Response>,
{
    type Request = S::Request;
    type Response = T::Tracked;
    type Error = S::Error;
    type Future = TrackFuture<S::Future, T, Tracker>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        T::track_future(self.tracker(), self.service.call(req))
    }
}

// ===== impl Tracker =====

impl Drop for Tracker {
    fn drop(&mut self) {
        unimplemented!()
    }
}
