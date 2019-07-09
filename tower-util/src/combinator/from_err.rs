use super::*;

/// `Service` analog to the `futures::future::FromErr` combinator.
pub struct FromErr<T, E> {
    inner: T,
    _err_marker: std::marker::PhantomData<E>,
}

impl<T, E> FromErr<T, E> {
    pub const fn new(service: T) -> Self {
        Self {
            inner: service,
            _err_marker: std::marker::PhantomData,
        }
    }
}

impl<Request, T, E> Service<Request> for FromErr<T, E>
where
    T: Service<Request>,
    E: From<T::Error>,
{
    type Response = T::Response;
    type Error = E;
    type Future = future::FromErr<T::Future, E>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(std::convert::Into::into)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        self.inner.call(req).from_err()
    }
}

pub struct FromErrLayer<E> {
    _err_marker: std::marker::PhantomData<E>,
}

impl<E> FromErrLayer<E> {
    pub const fn new() -> Self {
        Self {
            _err_marker: std::marker::PhantomData,
        }
    }
}

impl<E, T> Layer<T> for FromErrLayer<E> {
    type Service = FromErr<T, E>;

    fn layer(&self, service: T) -> Self::Service {
        FromErr::new(service)
    }
}
