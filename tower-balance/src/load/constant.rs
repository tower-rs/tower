use futures::Poll;
use tower::Service;

use {Load, Loaded};

/// Wraps a `Service` so that `Loaded::poll_load` returns a constant `Load`.
pub struct Constant<S> {
    inner: S,
    load: Load,
}

// ===== impl Constant =====

impl<S> Constant<S> {
    pub fn new(inner: S, load: Load) -> Self {
        Self {
            inner,
            load,
        }
    }

    pub fn min(inner: S) -> Self {
        Self::new(inner, Load::MIN)
    }

    pub fn max(inner: S) -> Self {
        Self::new(inner, Load::MAX)
    }
}

impl<S> Loaded for Constant<S> {
    fn load(&self) -> Load {
        self.load
    }
}

impl<S: Service> Service for Constant<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        self.inner.call(req)
    }
}
