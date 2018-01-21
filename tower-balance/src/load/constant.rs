use futures::Poll;
use tower::Service;

use {Load, Loaded};

/// Wraps a type so that `Loaded::load` returns a constant `Load`.
pub struct Constant<T> {
    inner: T,
    load: Load,
}

// ===== impl Constant =====

impl<T> Constant<T> {
    pub fn new(inner: T, load: Load) -> Self {
        Self {
            inner,
            load,
        }
    }
}

impl<T> Loaded for Constant<T> {
    fn load(&self) -> Load {
        self.load
    }
}

/// Proxies `Service`.
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
