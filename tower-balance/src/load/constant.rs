use futures::{Async, Poll};
use std::marker::PhantomData;
use tower::Service;

use {Load, PollLoad, WithLoad};

/// Wraps a `Service` so that `PollLoad::poll_load` returns a constant `Load`.
pub struct Constant<S: Service> {
    inner: S,
    load: Load,
}

/// Wraps `Service`s with `Constant`.
pub struct WithConstant<S: Service> {
    load: Load,
    _p: PhantomData<S>,
}

// ===== impl Constant =====

impl<S: Service> Constant<S> {
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

impl<S: Service> PollLoad for Constant<S> {
    fn poll_load(&mut self) -> Poll<Load, S::Error> {
        try_ready!(self.inner.poll_ready());
        Ok(Async::Ready(self.load))
    }
}

// ===== impl WithConstant =====

impl<S: Service> WithConstant<S> {
    pub fn new(load: Load) -> Self {
        WithConstant { load, _p: PhantomData }
    }

    pub fn min() -> Self {
        Self::new(Load::MIN)
    }

    pub fn max() -> Self {
        Self::new(Load::MAX)
    }
}

impl<S: Service> Clone for WithConstant<S> {
    fn clone(&self) -> Self {
        Self {
            load: self.load,
            _p: PhantomData,
        }
    }
}

impl<S: Service> Copy for WithConstant<S> {}

impl<S: Service> WithLoad for WithConstant<S> {
    type Service = S;
    type PollLoad = Constant<S>;

    fn with_load(&self, from: S) -> Self::PollLoad {
        Constant::new(from, self.load)
    }
}
