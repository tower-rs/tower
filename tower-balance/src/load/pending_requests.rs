use futures::{Future, Poll, Async};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::Service;

use {Load, Loaded};

/// Expresses `Load` based on the number of currently-pending requests.
///
/// The `Load` value approaches `Load::MAX` as the number of pending requests increases.
pub struct PendingRequests<S> {
    inner: S,
    pending: Arc<AtomicUsize>,
    factor: f64,
}

/// Wraps a response future so that it is tracked by `Pendingequests`.
pub struct ResponseFuture<S: Service> {
    inner: S::Future,
    pending: Option<Arc<AtomicUsize>>,
}

// ===== impl PendingRequests =====

impl<S: Service> PendingRequests<S> {
    pub fn new(inner: S, factor: f64) -> Self {
        assert!(0.0 < factor && factor < 1.0, "factor must be on (0.0, 1.0)");
        Self {
            inner,
            factor,
            pending: Arc::new(AtomicUsize::new(0)),
        }
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
        self.pending.fetch_add(1, Ordering::SeqCst);
        ResponseFuture {
            inner: self.inner.call(req),
            pending: Some(self.pending.clone()),
        }
    }
}

impl<S: Service> Loaded for PendingRequests<S> {
    fn load(&self) -> Load {
        // Calculate a load that approaches 1.0 as the number of pending requests
        // increases.
        let mut load = 0.0;
        for _ in 0..self.pending.load(Ordering::SeqCst) {
            load += (1.0 - load) * self.factor;
        }

        load.into()
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
                if let Some(p) = self.pending.take() {
                    p.fetch_sub(1, Ordering::SeqCst);
                }
                poll
            }
        }
    }
}
