use futures::{Async, Future, Poll};
use tower_service::Service;

/// Service for the `map` combinator, changing the type of a service's response.
///
/// This is created by the `ServiceExt::map` method.
pub struct Map<T, F, R>
where
    T: Service,
    F: Fn(T::Response) -> R + Clone,
{
    service: T,
    f: F,
}

impl<T, F, R> Map<T, F, R>
where
    T: Service,
    F: Fn(T::Response) -> R + Clone,
{
    /// Create new `Map` combinator
    pub fn new(service: T, f: F) -> Self {
        Map { service, f }
    }
}

impl<T, F, R> Clone for Map<T, F, R>
where
    T: Service + Clone,
    F: Fn(T::Response) -> R + Clone,
{
    fn clone(&self) -> Self {
        Map {
            service: self.service.clone(),
            f: self.f.clone(),
        }
    }
}

impl<T, F, R> Service for Map<T, F, R>
where
    T: Service,
    F: Fn(T::Response) -> R + Clone,
{
    type Request = T::Request;
    type Response = R;
    type Error = T::Error;
    type Future = MapFuture<T, F, R>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        MapFuture::new(self.service.call(req), self.f.clone())
    }
}

pub struct MapFuture<T, F, R>
where
    T: Service,
    F: Fn(T::Response) -> R,
{
    f: F,
    fut: T::Future,
}

impl<T, F, R> MapFuture<T, F, R>
where
    T: Service,
    F: Fn(T::Response) -> R,
{
    fn new(fut: T::Future, f: F) -> Self {
        MapFuture { f, fut }
    }
}

impl<T, F, R> Future for MapFuture<T, F, R>
where
    T: Service,
    F: Fn(T::Response) -> R,
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

    impl Service for Srv {
        type Request = ();
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
