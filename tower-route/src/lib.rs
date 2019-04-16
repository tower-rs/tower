extern crate futures;
extern crate tower_service;
extern crate void;

use futures::{future::Either, Async, Poll};
use tower_service::Service;

pub trait Accepts<Request> {
    fn accepts(&self, request: &Request) -> bool;
}

pub struct Route<A, B> {
    service: A,
    other: B,
}

impl<Request, A, B> Service<Request> for Route<A, B>
where
    A: Service<Request> + Accepts<Request>,
    B: Service<Request, Response = A::Response, Error = A::Error>,
{
    type Response = A::Response;
    type Error = A::Error;
    type Future = Either<A::Future, B::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(Async::Ready(()))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        if self.service.accepts(&request) {
            Either::A(self.service.call(request))
        } else {
            Either::B(self.other.call(request))
        }
    }
}
