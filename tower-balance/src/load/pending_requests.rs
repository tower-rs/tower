use futures::{Async, Poll};
use std::marker::PhantomData;
use std::sync::Arc;
use tower_discover::{Change, Discover};
use tower_service::Service;

use Load;
use super::track::{Track, TrackFuture, NoTrack};

/// Expresses load based on the number of currently-pending requests.
#[derive(Debug)]
pub struct PendingRequests<S, A = NoTrack>
where
    S: Service,
    A: Track<Tracker, S::Response>,
{
    service: S,
    track: Arc<()>,
    _p: PhantomData<A>,
}

/// Wraps `inner`'s services with `PendingRequests`.
#[derive(Debug)]
pub struct WithPendingRequests<D, A = NoTrack>
where
    D: Discover,
    A: Track<Tracker, D::Response>,
{
    discover: D,
    _p: PhantomData<A>,
}

/// Represents the number of currently-pending requests to a given service.
#[derive(Clone, Copy, Debug, Default, PartialOrd, PartialEq, Ord, Eq)]
pub struct Count(usize);

#[derive(Debug)]
pub struct Tracker(Arc<()>);

// ===== impl PendingRequests =====

impl<S, A> PendingRequests<S, A>
where
    S: Service,
    A: Track<Tracker, S::Response>,
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

    fn handle(&self) -> Tracker {
        Tracker(self.track.clone())
    }
}

impl<S, A> Load for PendingRequests<S, A>
where
    S: Service,
    A: Track<Tracker, S::Response>,
{
    type Metric = Count;

    fn load(&self) -> Count {
        Count(self.count())
    }
}

impl<S, A> Service for PendingRequests<S, A>
where
    S: Service,
    A: Track<Tracker, S::Response>,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = TrackFuture<S::Future, Tracker, A>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        A::track_future(self.handle(), self.service.call(req))
    }
}

// ===== impl WithPendingRequests =====

impl<D> WithPendingRequests<D, NoTrack>
where
    D: Discover,
{
    pub fn new(discover: D) -> Self {
        Self {
            discover,
            _p: PhantomData,
        }
    }
}

impl<D, A> WithPendingRequests<D, A>
where
    D: Discover,
    A: Track<Tracker, D::Response>,
{
    pub fn trackes<B>(self) -> WithPendingRequests<D, B>
    where
        B: Track<Tracker, D::Response>,
    {
        WithPendingRequests {
            discover: self.discover,
            _p: PhantomData,
        }
    }
}

impl<D, A> Discover for WithPendingRequests<D, A>
where
    D: Discover,
    A: Track<Tracker, D::Response>,
{
    type Key = D::Key;
    type Request = D::Request;
    type Response = D::Response;
    type Error = D::Error;
    type Service = PendingRequests<D::Service, A>;
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
