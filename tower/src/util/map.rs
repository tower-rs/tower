use futures::{Async, Future, Poll};
use tower_service::Service;

use std::marker::PhantomData;

/// Service for the `map` combinator, changing the type of a service's response.
///
/// This is created by the `ServiceExt::map` method.
pub struct Map<T, F, R> {
    service: T,
    f: F,
    _p: PhantomData<fn() -> R>,
}

impl<T, F, R> Map<T, F, R> {
    /// Create new `Map` combinator
    pub fn new<Request>(service: T, f: F) -> Self
    where
        T: Service<Request>,
        F: Fn(T::Response) -> R + Clone,
    {
        Map {
            service,
            f,
            _p: PhantomData,
        }
    }
}

impl<T, F, R> Clone for Map<T, F, R>
where
    T: Clone,
    F: Clone,
{
    fn clone(&self) -> Self {
        Map {
            service: self.service.clone(),
            f: self.f.clone(),
            _p: PhantomData,
        }
    }
}

impl<T, F, R, Request> Service<Request> for Map<T, F, R>
where
    T: Service<Request>,
    F: Fn(T::Response) -> R + Clone,
{
    type Response = R;
    type Error = T::Error;
    type Future = MapFuture<T::Future, F, R>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        MapFuture::new(self.service.call(req), self.f.clone())
    }
}

pub struct MapFuture<T, F, R>
where
    T: Future,
    F: Fn(T::Item) -> R,
{
    f: F,
    fut: T,
}

impl<T, F, R> MapFuture<T, F, R>
where
    T: Future,
    F: Fn(T::Item) -> R,
{
    fn new(fut: T, f: F) -> Self {
        MapFuture { f, fut }
    }
}

impl<T, F, R> Future for MapFuture<T, F, R>
where
    T: Future,
    F: Fn(T::Item) -> R,
{
    type Item = R;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let resp = try_ready!(self.fut.poll());
        Ok(Async::Ready((self.f)(resp)))
    }
}

#[cfg(test)]
mod tests {
    use futures::future::{ok, FutureResult};

    use super::*;
    use ServiceExt;

    struct Srv;

    impl Service<()> for Srv {
        type Response = ();
        type Error = ();
        type Future = FutureResult<(), ()>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Ok(Async::Ready(()))
        }

        fn call(&mut self, _: ()) -> Self::Future {
            ok(())
        }
    }

    #[test]
    fn test_poll_ready() {
        let mut srv = Srv.map(|_| "ok");
        let res = srv.poll_ready();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), Async::Ready(()));
    }

    #[test]
    fn test_call() {
        let mut srv = Srv.map(|_| "ok");
        let res = srv.call(()).poll();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), Async::Ready("ok"));
    }
}
