use futures::{IntoFuture, Poll};
use tower_service::Service;

/// Returns a new `ServiceFn` with the given closure.
pub fn service_fn<T>(f: T) -> ServiceFn<T> {
    ServiceFn { f }
}

/// A `Service` implemented by a closure.
#[derive(Copy, Clone, Debug)]
pub struct ServiceFn<T> {
    f: T,
}

impl<T, F, Request> Service<Request> for ServiceFn<T>
where
    T: FnMut(Request) -> F,
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
