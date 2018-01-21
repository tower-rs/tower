use futures::{Future, Poll, Async};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::Service;

use {Load, Loaded};

/// Expresses `Load` based on the number of currently-pending requests.
///
/// The `Load` value approaches `Load::MAX` as the number of pending requests increases
/// according to the equation:
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

struct Pending(Arc<AtomicUsize>);

/// Wraps a response future so that it is tracked by `PendingRequests`.
///
/// When the inner future completes, either with a value or an error, `pending` is
/// decremented.
pub struct ResponseFuture<S: Service> {
    inner: S::Future,
    pending: Option<Pending>,
}

// ===== impl PendingRequests =====

impl<T> PendingRequests<T> {
    const FACTOR: f64 = 10.0;

    pub fn new(inner: T) -> Self {
        Self {
            inner,
            pending: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl<S: Service> Loaded for PendingRequests<S> {
    fn load(&self) -> Load {
        let n = self.pending.load(Ordering::SeqCst) as f64;
        Load::new(n / (n + Self::FACTOR))
    }
}

/// Proxies `Service`.
impl<S: Service> Service for PendingRequests<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        // Increment the pending countimmediately.
        self.pending.fetch_add(1, Ordering::SeqCst);

        // It will be decremented when this is dropped.
        let p = Pending(self.pending.clone());

        ResponseFuture {
            inner: self.inner.call(req),
            pending: Some(p),
        }
    }
}

// ===== impl Pending =====

impl Drop for Pending {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
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
                drop(self.pending.take());
                poll
            }
        }
    }
}
