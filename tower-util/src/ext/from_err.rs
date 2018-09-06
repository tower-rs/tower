use std::marker::PhantomData;

use futures::{Future, Poll};
use tower_service::Service;

/// Service for the `from_err` combinator, changing the error type of a service.
///
/// This is created by the `ServiceExt::from_err` method.
pub struct FromErr<A, E>
where
    A: Service,
{
    service: A,
    _e: PhantomData<E>,
}

impl<A: Service, E: From<A::Error>> FromErr<A, E> {
    pub(crate) fn new(service: A) -> Self {
        FromErr {
            service,
            _e: PhantomData,
        }
    }
}

impl<A, E> Clone for FromErr<A, E>
where
    A: Service + Clone,
{
    fn clone(&self) -> Self {
        FromErr {
            service: self.service.clone(),
            _e: PhantomData,
        }
    }
}

impl<A, E> Service for FromErr<A, E>
where
    A: Service,
    E: From<A::Error>,
{
    type Request = A::Request;
    type Response = A::Response;
    type Error = E;
    type Future = FromErrFuture<A, E>;

    fn poll_ready(&mut self) -> Poll<(), E> {
        Ok(self.service.poll_ready().map_err(E::from)?)
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        FromErrFuture {
            fut: self.service.call(req),
            f: PhantomData,
        }
    }
}

pub struct FromErrFuture<A: Service, E> {
    fut: A::Future,
    f: PhantomData<E>,
}

impl<A, E> Future for FromErrFuture<A, E>
where
    A: Service,
    E: From<A::Error>,
{
    type Item = A::Response;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.fut.poll().map_err(E::from)
    }
}
