use futures::{Future, Poll, Async};
use futures::sync::oneshot;
use std::collections::VecDeque;
use std::marker::PhantomData;
use tower::Service;

use {Load, PollLoad, WithLoad};

/// Expresses `Load` based on the number of currently-pending requests.
///
/// The `Load` value approaches `Load::MAX` as the number of pending requests increases.
pub struct PendingRequests<S: Service> {
    inner: S,
    pending: VecDeque<oneshot::Receiver<()>>,
    factor: f64,
}

/// Wraps `Service`s with `PendingRequests`.
pub struct WithPendingRequests<S: Service> {
    factor: f64,
    _p: PhantomData<S>
}

/// Wraps a response future so that it is tracked by `Pendingequests`.
pub struct ResponseFuture<S: Service> {
    inner: S::Future,
    done: Option<oneshot::Sender<()>>,
}

// ===== impl PendingRequests =====

impl<S: Service> PendingRequests<S> {
    pub fn new(inner: S, factor: f64) -> Self {
        assert!(0.0 < factor && factor < 1.0, "factor must be on (0.0, 1.0)");
        Self {
            inner,
            factor,
            pending: VecDeque::default(),
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
        let (tx, rx) = oneshot::channel();
        self.pending.push_back(rx);
        ResponseFuture {
            inner: self.inner.call(req),
            done: Some(tx),
        }
    }
}

impl<S: Service> PollLoad for PendingRequests<S> {
    fn poll_load(&mut self) -> Poll<Load, S::Error> {
        try_ready!(self.poll_ready());

        // Calculate a load that approaches 1.0 as the number of pending requests
        // increases.
        let mut load = 0.0;
        for _ in 0..self.pending.len() {
            let mut rx = self.pending.pop_front().expect("out of range");

            if let Ok(Async::NotReady) = rx.poll() {
                self.pending.push_back(rx);
                load += (1.0 - load) * self.factor;
            }
        }

        Ok(Async::Ready(load.into()))
    }
}

// ===== impl ResponseFuture =====

impl<S: Service> Future for ResponseFuture<S> {
    type Item = S::Response;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let item = try_ready!(self.inner.poll());
        if let Some(tx) = self.done.take() {
            let _ = tx.send(());
        }
        Ok(Async::Ready(item))
    }
}

// ===== impl WithPendingRequests =====

impl<S: Service> Clone for WithPendingRequests<S> {
    fn clone(&self) -> Self {
        Self {
            factor: self.factor,
            _p: PhantomData,
        }
    }
}

impl<S: Service> Copy for WithPendingRequests<S> {}

impl<S: Service> WithPendingRequests<S> {
    fn new(factor: f64) -> Self {
        assert!(0.0 < factor && factor < 1.0, "factor out of range");
        Self {
            factor,
            _p: PhantomData,
        }
    }
}

impl<S: Service> Default for WithPendingRequests<S> {
    fn default() -> Self {
        Self::new(0.5)
    }
}

impl<S: Service> WithLoad for WithPendingRequests<S> {
    type Service = S;
    type PollLoad = PendingRequests<S>;

    fn with_load(&self, from: S) -> Self::PollLoad {
        PendingRequests::new(from, self.factor)
    }
}
