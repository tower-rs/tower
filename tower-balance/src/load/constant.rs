use futures::{Async, Poll};
use tower::Service;
use tower_discover::{Change, Discover};

use {Load, Loaded};

/// Wraps a type so that `Loaded::load` returns a constant value.
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

/// Proxies `Discover` such that all changes are wrapped with a constant load.
impl<D: Discover> Discover for Constant<D> {
    type Key = D::Key;
    type Request = D::Request;
    type Response = D::Response;
    type Error = D::Error;
    type Service = Constant<D::Service>;
    type DiscoverError = D::DiscoverError;

    /// Yields the next discovery change set.
    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::DiscoverError> {
        use self::Change::*;

        let change = match try_ready!(self.inner.poll()) {
            Insert(k, svc) => Insert(k, Constant::new(svc, self.load)),
            Remove(k) => Remove(k),
        };

        Ok(Async::Ready(change))
    }
}
