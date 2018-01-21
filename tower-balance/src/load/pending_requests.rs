use futures::{Future, Poll, Async};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::Service;

use {Load, Loaded};

/// Expresses `Load` based on the number of currently-pending requests.
///
/// The `Load` value approaches `Load::MAX` as the number of pending requests increases,
/// according to:
///
///              pending
///     load = ------------
///            pending + 10
///
#[derive(Debug, Clone)]
pub struct PendingRequests<T> {
    inner: T,
    pending: Arc<AtomicUsize>,
}

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

// ===== impl PendingRequests =====

impl<T> PendingRequests<T> {
    // When `pending` is 10, load will be 0.5.
    const MIDPOINT: f64 = 10.0;

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

impl<S: Service> Loaded for PendingRequests<S> {
    fn load(&self) -> Load {
        let n = self.pending.load(Ordering::SeqCst) as f64;
        Load::new(n / (n + Self::MIDPOINT))
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
