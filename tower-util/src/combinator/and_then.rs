use super::*;

/// `Service` analog to the `futures::future::AndThen` combinator.
pub struct AndThen<T, F> {
    inner: T,
    f: F,
}

impl<T, F> AndThen<T, F> {
    pub const fn new(service: T, f: F) -> Self {
        Self { inner: service, f }
    }
}

impl<Request, T, F, U> Service<Request> for AndThen<T, F>
where
    T: Service<Request>,
    F: Copy + FnOnce(T::Response) -> U,
    U: IntoFuture<Error = T::Error>,
{
    type Response = U::Item;
    type Error = T::Error;
    type Future = future::AndThen<T::Future, U, F>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let f = self.f;
        self.inner.call(req).and_then(f)
    }
}

pub struct AndThenLayer<F> {
    f: F,
}

impl<F> AndThenLayer<F> {
    pub const fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F, T> Layer<T> for AndThenLayer<F>
where
    F: Copy,
{
    type Service = AndThen<T, F>;

    fn layer(&self, service: T) -> Self::Service {
        AndThen::new(service, self.f)
    }
}
