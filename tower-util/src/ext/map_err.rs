use futures::{Future, Poll};
use tower_service::Service;

/// Service for the `map_err` combinator, changing the type of a service's error.
///
/// This is created by the `ServiceExt::map_err` method.
pub struct MapErr<T, F, E>
where
    T: Service,
    F: Fn(T::Error) -> E + Clone,
{
    service: T,
    f: F,
}

impl<T, F, E> MapErr<T, F, E>
where
    T: Service,
    F: Fn(T::Error) -> E + Clone,
{
    /// Create new `MapErr` combinator
    pub fn new(service: T, f: F) -> Self {
        MapErr { service, f }
    }
}

impl<T, F, E> Service for MapErr<T, F, E>
where
    T: Service,
    F: Fn(T::Error) -> E + Clone,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = E;
    type Future = MapErrFuture<T, F, E>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready().map_err(&self.f)
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        MapErrFuture::new(self.service.call(req), self.f.clone())
    }
}

pub struct MapErrFuture<T, F, E>
where
    T: Service,
    F: Fn(T::Error) -> E,
{
    f: F,
    fut: T::Future,
}

impl<T, F, E> MapErrFuture<T, F, E>
where
    T: Service,
    F: Fn(T::Error) -> E,
{
    fn new(fut: T::Future, f: F) -> Self {
        MapErrFuture { f, fut }
    }
}

impl<T, F, E> Future for MapErrFuture<T, F, E>
where
    T: Service,
    F: Fn(T::Error) -> E,
{
    type Item = T::Response;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.fut.poll().map_err(&self.f)
    }
}
