use std::{fmt, marker::PhantomData};

use futures_core::ready;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// Future yielding a `Service` once the service is ready to process a request
///
/// `Ready` values are produced by `ServiceExt::ready`.
pub struct Ready<'a, T, Request> {
    inner: &'a mut T,
    _p: PhantomData<fn() -> Request>,
}

// Safety: This is safe because `Services`'s are always `Unpin`.
impl<'a, T, Request> Unpin for Ready<'a, T, Request> {}

impl<'a, T, Request> Ready<'a, T, Request>
where
    T: Service<Request>,
{
    #[allow(missing_docs)]
    pub fn new(service: &'a mut T) -> Self {
        Ready {
            inner: service,
            _p: PhantomData,
        }
    }
}

impl<'a, T, Request> Future for Ready<'a, T, Request>
where
    T: Service<Request>,
{
    type Output = Result<(), T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        ready!(self.inner.poll_ready(cx))?;

        Poll::Ready(Ok(()))
    }
}

impl<'a, T, Request> fmt::Debug for Ready<'a, T, Request>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Ready").field("inner", &self.inner).finish()
    }
}
