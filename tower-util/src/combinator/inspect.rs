use super::*;

/// `Service` analog to the `futures::future::Inspect` combinator.
pub struct Inspect<T, F> {
    inner: T,
    f: F,
}

impl<T, F> Inspect<T, F> {
    pub const fn new(service: T, f: F) -> Self {
        Self { inner: service, f }
    }
}

impl<Request, T, F> Service<Request> for Inspect<T, F>
where
    T: Service<Request>,
    F: Copy + FnOnce(&T::Response),
{
    type Response = T::Response;
    type Error = T::Error;
    type Future = future::Inspect<T::Future, F>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let f = self.f;
        self.inner.call(req).inspect(f)
    }
}

pub struct InspectLayer<F> {
    f: F,
}

impl<F> InspectLayer<F> {
    pub const fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F, T> Layer<T> for InspectLayer<F>
where
    F: Copy,
{
    type Service = Inspect<T, F>;

    fn layer(&self, service: T) -> Self::Service {
        Inspect::new(service, self.f)
    }
}
