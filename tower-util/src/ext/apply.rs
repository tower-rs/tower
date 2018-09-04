use std::marker::PhantomData;

use futures::{Future, IntoFuture, Poll};
use tower_service::Service;

/// `Apply` service combinator
pub struct Apply<T, F, R, Req>
{
    service: T,
    f: F,
    _r: PhantomData<Fn(Req) -> R>,
}

impl<T, F, R, Req> Apply<T, F, R, Req>
where
    T: Service<Error=R::Error> + Clone,
    F: Fn(Req, T) -> R,
    R: IntoFuture,
{
    /// Create new `Apply` combinator
    pub fn new(f: F, service: T) -> Self {
        Self {
            service,
            f,
            _r: PhantomData,
        }
    }
}

impl<T, F, R, Req> Service for Apply<T, F, R, Req>
where
    T: Service<Error=R::Error> + Clone,
    F: Fn(Req, T) -> R,
    R: IntoFuture,
{
    type Request = Req;
    type Response = <R::Future as Future>::Item;
    type Error = <R::Future as Future>::Error;
    type Future = R::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        let service = self.service.clone();
        (self.f)(req, service).into_future()
    }
}
