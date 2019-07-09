use super::*;

/// `Service` analog to the `futures::future::Map` combinator.
pub struct Map<T, F> {
    inner: T,
    f: F,
}

impl<T, F> Map<T, F> {
    pub const fn new(service: T, f: F) -> Self {
        Self { inner: service, f }
    }
}

impl<Request, T, F, U> Service<Request> for Map<T, F>
where
    T: Service<Request>,
    F: Copy + Fn(T::Response) -> U,
{
    type Response = U;
    type Error = T::Error;
    type Future = future::Map<T::Future, F>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let f = self.f;
        self.inner.call(req).map(f)
    }
}

pub struct MapLayer<F> {
    f: F,
}

impl<F> MapLayer<F> {
    pub const fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F, T> Layer<T> for MapLayer<F>
where
    F: Copy,
{
    type Service = Map<T, F>;

    fn layer(&self, service: T) -> Self::Service {
        Map::new(service, self.f)
    }
}
