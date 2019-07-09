use super::*;

/// `Service` analog to the `futures::future::MapErr` combinator.
pub struct MapErr<T, F> {
    inner: T,
    f: F,
}

impl<T, F> MapErr<T, F> {
    /// Create a new mapped service, wrapping the inner service `service`
    /// and applying `f` to all errors.
    pub const fn new(service: T, f: F) -> Self {
        Self { inner: service, f }
    }
}

impl<Request, T, F, U> Service<Request> for MapErr<T, F>
where
    T: Service<Request>,
    F: Copy + Fn(T::Error) -> U,
{
    type Response = T::Response;
    type Error = U;
    type Future = future::MapErr<T::Future, F>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let f = self.f;
        self.inner.poll_ready().map_err(f)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let f = self.f;
        self.inner.call(req).map_err(f)
    }
}

pub struct MapErrLayer<F> {
    f: F,
}

impl<F> MapErrLayer<F> {
    pub const fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F, T> Layer<T> for MapErrLayer<F>
where
    F: Copy,
{
    type Service = MapErr<T, F>;

    fn layer(&self, service: T) -> Self::Service {
        MapErr::new(service, self.f)
    }
}
