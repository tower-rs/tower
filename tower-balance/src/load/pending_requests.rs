use futures::{Async, Poll};
use std::marker::PhantomData;
use std::sync::Arc;
use tower_discover::{Change, Discover};
use tower_service::Service;

use Load;
use super::{Measure, MeasureFuture, NoMeasure};

/// Expresses load based on the number of currently-pending requests.
#[derive(Debug)]
pub struct PendingRequests<S, M = NoMeasure>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    service: S,
    track: Arc<()>,
    _p: PhantomData<M>,
}

/// Wraps `inner`'s services with `PendingRequests`.
#[derive(Debug)]
pub struct WithPendingRequests<D, M = NoMeasure>
where
    D: Discover,
    M: Measure<Instrument, D::Response>,
{
    discover: D,
    _p: PhantomData<M>,
}

/// Represents the number of currently-pending requests to a given service.
#[derive(Clone, Copy, Debug, Default, PartialOrd, PartialEq, Ord, Eq)]
pub struct Count(usize);

#[derive(Debug)]
pub struct Instrument(Arc<()>);

// ===== impl PendingRequests =====

impl<S, M> PendingRequests<S, M>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    pub fn new(service: S) -> Self {
        Self {
            service,
            track: Arc::new(()),
            _p: PhantomData,
        }
    }

    pub fn count(&self) -> usize {
        // Count the number of references that aren't `self.track`.
        Arc::strong_count(&self.track) - 1
    }

    fn instrument(&self) -> Instrument {
        Instrument(self.track.clone())
    }
}

impl<S, M> Load for PendingRequests<S, M>
where
    S: Service,
    M: Measure<Instrument, S::Response>,
{
    type Metric = Count;

    fn load(&self) -> Count {
        Count(self.count())
    }
}

impl<S, M> Service for PendingRequests<S, M>
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

// ===== impl WithPendingRequests =====

impl<D> WithPendingRequests<D, NoMeasure>
where
    D: Discover,
{
    pub fn new(discover: D) -> Self {
        Self {
            discover,
            _p: PhantomData,
        }
    }

    pub fn measured<M>(self) -> WithPendingRequests<D, M>
    where
        M: Measure<Instrument, D::Response>,
    {
        WithPendingRequests {
            discover: self.discover,
            _p: PhantomData,
        }
    }
}

impl<D, M> Discover for WithPendingRequests<D, M>
where
    D: Discover,
    M: Measure<Instrument, D::Response>,
{
    type Key = D::Key;
    type Request = D::Request;
    type Response = M::Measured;
    type Error = D::Error;
    type Service = PendingRequests<D::Service, M>;
    type DiscoverError = D::DiscoverError;

    /// Yields the next discovery change set.
    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::DiscoverError> {
        use self::Change::*;

        let change = match try_ready!(self.discover.poll()) {
            Insert(k, svc) => Insert(k, PendingRequests::new(svc)),
            Remove(k) => Remove(k),
        };

        Ok(Async::Ready(change))
    }
}
