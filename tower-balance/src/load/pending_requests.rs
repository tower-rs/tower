use futures::{Future, Poll, Async};
use std::ops;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::Service;
use tower_discover::{Change, Discover};

use {Load, Weight};

/// Expresses load based on the number of currently-pending requests.
#[derive(Debug, Clone)]
pub struct PendingRequests<T> {
    inner: T,
    pending: Arc<AtomicUsize>,
}

/// Wraps `inner`'s services with `PendingRequests`.
pub struct WithPendingRequests<D: Discover> {
    inner: D,
}

/// Represents the number of currently-pending requests to a given service.
#[derive(Clone, Copy, Debug, Default, PartialOrd, PartialEq, Ord, Eq)]
pub struct Count(usize);

/// Ensures that `pending` is decremented.
struct Handle {
    pending: Arc<AtomicUsize>,
}

/// Wraps a response future so that it is tracked by `PendingRequests`.
///
/// When the inner future completes, either with a value or an error, `pending` is
/// decremented.
pub struct ResponseFuture<S: Service> {
    inner: S::Future,
    pending: Option<Handle>,
}

// ===== impl Count =====

impl ops::Div<Weight> for Count {
    type Output = f64;

    fn div(self, weight: Weight) -> f64 {
        self.0 / weight
    }
}

// ===== impl PendingRequests =====

impl<T> PendingRequests<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            pending: Arc::new(AtomicUsize::new(0)),
        }
    }

    // Increments the pending count immediately and returns a `Handle` that will decrement
    // the count when dropped.
    fn incr_pending(&self) -> Handle {
        let pending = self.pending.clone();
        pending.fetch_add(1, Ordering::SeqCst);
        Handle { pending }
    }
}

impl<S: Service> Load for PendingRequests<S> {
    type Metric = Count;

    fn load(&self) -> Count {
        Count(self.pending.load(Ordering::SeqCst))
    }
}

impl<S: Service> Service for PendingRequests<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        ResponseFuture {
            pending: Some(self.incr_pending()),
            inner: self.inner.call(req),
        }
    }
}

// ===== impl WithPendingRequests =====

impl<D: Discover> WithPendingRequests<D> {
    pub fn new(inner: D) -> Self {
        Self { inner }
    }
}

impl<D: Discover> Discover for WithPendingRequests<D> {
    type Key = D::Key;
    type Request = D::Request;
    type Response = D::Response;
    type Error = D::Error;
    type Service = PendingRequests<D::Service>;
    type DiscoverError = D::DiscoverError;

    /// Yields the next discovery change set.
    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::DiscoverError> {
        use self::Change::*;

        let change = match try_ready!(self.inner.poll()) {
            Insert(k, svc) => Insert(k, PendingRequests::new(svc)),
            Remove(k) => Remove(k),
        };

        Ok(Async::Ready(change))
    }
}

// ===== impl Pending =====

impl Drop for Handle {
    fn drop(&mut self) {
        self.pending.fetch_sub(1, Ordering::SeqCst);
    }
}

// ===== impl ResponseFuture =====

impl<S: Service> Future for ResponseFuture<S> {
    type Item = S::Response;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.inner.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            poll => {
                // Decrements the pending count as needed.
                drop(self.pending.take());
                poll
            }
        }
    }
}
