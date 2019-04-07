use std::fmt;
use std::marker::PhantomData;

use futures::{Future, Poll};
use tower_service::Service;

/// Future yielding a `Service` once the service is ready to process a request
///
/// `Ready` values are produced by `ServiceExt::ready`.
pub struct Ready<T, Request> {
    inner: Option<T>,
    _p: PhantomData<fn() -> Request>,
}

impl<T, Request> Ready<T, Request>
where
    T: Service<Request>,
{
    pub fn new(service: T) -> Self {
        Ready {
            inner: Some(service),
            _p: PhantomData,
        }
    }
}

impl<T, Request> Future for Ready<T, Request>
where
    T: Service<Request>,
{
    type Item = T;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<T, T::Error> {
        match self.inner {
            Some(ref mut service) => {
                let _ = try_ready!(service.poll_ready());
            }
            None => panic!("called `poll` after future completed"),
        }

        Ok(self.inner.take().unwrap().into())
    }
}

impl<T, Request> fmt::Debug for Ready<T, Request>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ready").field("inner", &self.inner).finish()
    }
}
