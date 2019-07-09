use super::*;

/// `Service` analog to the `futures::future::OrElse` combinator.
///
/// TODO: Currently restricted to not modifying the error type,
/// since we need `poll_ready` and `call` to have the same type.
/// But it doesn't make sense to `or_else` the `poll_ready` type.
pub struct OrElse<T, F> {
    inner: T,
    f: F,
}

impl<T, F> OrElse<T, F> {
    pub const fn new(service: T, f: F) -> Self {
        Self { inner: service, f }
    }
}

impl<Request, T, F, U> Service<Request> for OrElse<T, F>
where
    T: Service<Request>,
    F: Copy + FnOnce(T::Error) -> U,
    U: IntoFuture<Item = T::Response, Error = T::Error>,
{
    type Response = T::Response;
    type Error = T::Error;
    type Future = future::OrElse<T::Future, U, F>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let f = self.f;
        self.inner.call(req).or_else(f)
    }
}

pub struct OrElseLayer<F> {
    f: F,
}

impl<F> OrElseLayer<F> {
    pub const fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F, T> Layer<T> for OrElseLayer<F>
where
    F: Copy,
{
    type Service = OrElse<T, F>;

    fn layer(&self, service: T) -> Self::Service {
        OrElse::new(service, self.f)
    }
}
