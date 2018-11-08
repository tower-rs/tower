use futures::{IntoFuture, Poll};
use tower_service::Service;

/// A `Service` implemented by a closure.
pub struct ServiceFn<T> {
    f: T,
}

// ===== impl ServiceFn =====

impl<T> ServiceFn<T> {
    /// Returns a new `NewServiceFn` with the given closure.
    pub fn new(f: T) -> Self {
        ServiceFn { f }
    }
}

impl<T, F, Request> Service<Request> for ServiceFn<T>
where T: Fn(Request) -> F,
      F: IntoFuture,
{
    type Response = F::Item;
    type Error = F::Error;
    type Future = F::Future;

    fn poll_ready(&mut self) -> Poll<(), F::Error> {
        Ok(().into())
    }

    fn call(&mut self, req: Request) -> Self::Future {
        (self.f)(req).into_future()
    }
}
