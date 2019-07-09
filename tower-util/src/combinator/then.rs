use super::*;

/// `Service` analog to the `futures::future::Then` combinator.
///
/// TODO: Currently restricted to not modifying the error type,
/// since we need `poll_ready` and `call` to have the same type.
/// But it doesn't make sense to apply `then` to the `poll_ready` type.
pub struct Then<T, F> {
    inner: T,
    f: F,
}

impl<T, F> Then<T, F> {
    pub const fn new(service: T, f: F) -> Self {
        Self { inner: service, f }
    }
}

impl<Request, T, F, U> Service<Request> for Then<T, F>
where
    T: Service<Request>,
    F: Copy + FnOnce(Result<T::Response, T::Error>) -> U,
    U: IntoFuture<Error = T::Error>,
{
    type Response = U::Item;
    type Error = U::Error;
    type Future = future::Then<T::Future, U, F>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let f = self.f;
        self.inner.call(req).then(f)
    }
}

pub struct ThenLayer<F> {
    f: F,
}

impl<F> ThenLayer<F> {
    pub const fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F, T> Layer<T> for ThenLayer<F>
where
    F: Copy,
{
    type Service = Then<T, F>;

    fn layer(&self, service: T) -> Self::Service {
        Then::new(service, self.f)
    }
}
