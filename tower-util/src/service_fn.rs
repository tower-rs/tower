use futures::{Async, IntoFuture, Poll};
use tower_service::{Service, NewService};

use std::marker::PhantomData;

/// A `Service` implemented by a closure.
#[derive(Debug, Clone)]
pub struct ServiceFn<T, R> {
    f: T,
    // don't impose Sync on R
    _ty: PhantomData<fn() -> R>,
}

/// A `NewService` implemented by a closure.
pub struct NewServiceFn<T> {
    f: T,
}

// ===== impl ServiceFn =====

impl<T, R, S> ServiceFn<T, R>
where T: FnMut(R) -> S,
      S: IntoFuture,
{
    /// Create a new `ServiceFn` backed by the given closure
    pub fn new(f: T) -> Self {
        ServiceFn {
            f,
            _ty: PhantomData,
        }
    }
}

impl<T, R, S> Service for ServiceFn<T, R>
where T: FnMut(R) -> S,
      S: IntoFuture,
{
    type Request = R;
    type Response = S::Item;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(Async::Ready(()))
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        (self.f)(request).into_future()
    }
}

// ===== impl NewServiceFn =====

impl<T, N> NewServiceFn<T>
where T: Fn() -> N,
      N: Service,
{
    /// Returns a new `NewServiceFn` with the given closure.
    pub fn new(f: T) -> Self {
        NewServiceFn { f }
    }
}

impl<T, R, S> NewService for NewServiceFn<T>
where T: Fn() -> R,
      R: IntoFuture<Item = S>,
      S: Service,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Service = R::Item;
    type InitError = R::Error;
    type Future = R::Future;

    fn new_service(&self) -> Self::Future {
        (self.f)().into_future()
    }
}
