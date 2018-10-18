use futures::{Future, Poll};
use tower_service::Service;

use std::marker::PhantomData;

/// Service for the `map_err` combinator, changing the type of a service's error.
///
/// This is created by the `ServiceExt::map_err` method.
pub struct MapErr<T, F, E> {
    service: T,
    f: F,
    _p: PhantomData<E>,
}

impl<T, F, E> MapErr<T, F, E> {
    /// Create new `MapErr` combinator
    pub fn new<Request>(service: T, f: F) -> Self
    where
        T: Service<Request>,
        F: Fn(T::Error) -> E + Clone,
    {
        MapErr { service, f, _p: PhantomData }
    }
}

impl<T, F, E> Clone for MapErr<T, F, E>
where
    T: Clone,
    F: Clone,
{
    fn clone(&self) -> Self {
        MapErr {
            service: self.service.clone(),
            f: self.f.clone(),
            _p: PhantomData,
        }
    }
}

impl<T, F, E, Request> Service<Request> for MapErr<T, F, E>
where
    T: Service<Request>,
    F: Fn(T::Error) -> E + Clone,
{
    type Response = T::Response;
    type Error = E;
    type Future = MapErrFuture<T::Future, F, E>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready().map_err(&self.f)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        MapErrFuture::new(self.service.call(req), self.f.clone())
    }
}

pub struct MapErrFuture<T, F, E>
where
    T: Future,
    F: Fn(T::Error) -> E,
{
    f: F,
    fut: T,
}

impl<T, F, E> MapErrFuture<T, F, E>
where
    T: Future,
    F: Fn(T::Error) -> E,
{
    fn new(fut: T, f: F) -> Self {
        MapErrFuture { f, fut }
    }
}

impl<T, F, E> Future for MapErrFuture<T, F, E>
where
    T: Future,
    F: Fn(T::Error) -> E,
{
    type Item = T::Item;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.fut.poll().map_err(&self.f)
    }
}

#[cfg(test)]
mod tests {
    use futures::future::{err, FutureResult};

    use super::*;
    use ServiceExt;

    struct Srv;

    impl Service for Srv {
        type Request = ();
        type Response = ();
        type Error = ();
        type Future = FutureResult<(), ()>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Err(())
        }

        fn call(&mut self, _: ()) -> Self::Future {
            err(())
        }
    }

    #[test]
    fn test_poll_ready() {
        let mut srv = Srv.map_err(|_| "error");
        let res = srv.poll_ready();
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), "error");
    }

    #[test]
    fn test_call() {
        let mut srv = Srv.map_err(|_| "error");
        let res = srv.call(()).poll();
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), "error");
    }
}
