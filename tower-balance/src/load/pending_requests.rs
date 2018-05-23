use futures::{Async, Poll};
use std::marker::PhantomData;
use std::sync::Arc;
use tower_discover::{Change, Discover};
use tower_service::Service;

use Load;
use super::track::{Track, TrackFuture, NoTrack};

/// Expresses load based on the number of currently-pending requests.
#[derive(Debug)]
pub struct PendingRequests<S, T = NoTrack>
where
    S: Service,
    T: Track<Tracker, S::Response>,
{
    service: S,
    track: Arc<()>,
    _p: PhantomData<T>,
}

/// Wraps `inner`'s services with `PendingRequests`.
#[derive(Debug)]
pub struct WithPendingRequests<D, T = NoTrack>
where
    D: Discover,
    T: Track<Tracker, D::Response>,
{
    discover: D,
    _p: PhantomData<T>,
}

/// Represents the number of currently-pending requests to a given service.
#[derive(Clone, Copy, Debug, Default, PartialOrd, PartialEq, Ord, Eq)]
pub struct Count(usize);

#[derive(Debug)]
pub struct Tracker(Arc<()>);

// ===== impl PendingRequests =====

impl<S, T> PendingRequests<S, T>
where
    S: Service,
    T: Track<Tracker, S::Response>,
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

    fn tracker(&self) -> Tracker {
        Tracker(self.track.clone())
    }
}

impl<S, T> Load for PendingRequests<S, T>
where
    S: Service,
    T: Track<Tracker, S::Response>,
{
    type Metric = Count;

    fn load(&self) -> Count {
        Count(self.count())
    }
}

impl<S, T> Service for PendingRequests<S, T>
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

impl<D, T> WithPendingRequests<D, T>
where
    D: Discover,
    T: Track<Tracker, D::Response>,
{
    pub fn track_with<U>(self) -> WithPendingRequests<D, U>
    where
        U: Track<Tracker, D::Response>,
    {
        WithPendingRequests {
            discover: self.discover,
            _p: PhantomData,
        }
    }
}

impl<D, T> Discover for WithPendingRequests<D, T>
where
    D: Discover,
    T: Track<Tracker, D::Response>,
{
    type Key = D::Key;
    type Request = D::Request;
    type Response = T::Tracked;
    type Error = D::Error;
    type Service = PendingRequests<D::Service, T>;
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
