use std::marker::PhantomData;

use futures::{Async, Future, Poll};
use tower_service::Service;

/// Service for the `map` combinator, changing the type of a service's response.
///
/// This is created by the `ServiceExt::map` method.
pub struct Map<T, F, R> {
    service: T,
    f: F,
    r: PhantomData<R>,
}

impl<T, F, R> Map<T, F, R>
where
    T: Service,
    F: Fn(T::Response) -> R + Clone,
{
    /// Create new `Map` combinator
    pub fn new(service: T, f: F) -> Self {
        Map {
            service,
            f,
            r: PhantomData,
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
