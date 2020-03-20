use std::{fmt, marker::PhantomData};

use futures_core::ready;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// A future that yields a `Service` when it is ready to accept a request.
///
/// `ReadyAnd` values are produced by `ServiceExt::ready_and`.
pub struct ReadyAnd<'a, T, Request> {
    inner: Option<&'a mut T>,
    _p: PhantomData<fn() -> Request>,
}

// Safety: This is safe because `Services`'s are always `Unpin`.
impl<'a, T, Request> Unpin for ReadyAnd<'a, T, Request> {}

impl<'a, T, Request> ReadyAnd<'a, T, Request>
where
    T: Service<Request>,
{
    #[allow(missing_docs)]
    pub fn new(service: &'a mut T) -> Self {
        Self {
            inner: Some(service),
            _p: PhantomData,
        }
    }
}

impl<'a, T, Request> Future for ReadyAnd<'a, T, Request>
where
    T: Service<Request>,
{
    type Output = Result<&'a mut T, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        ready!(self
            .inner
            .as_mut()
            .expect("poll after Poll::Ready")
            .poll_ready(cx))?;

        Poll::Ready(Ok(self.inner.take().expect("poll after Poll::Ready")))
    }
}

impl<'a, T, Request> fmt::Debug for ReadyAnd<'a, T, Request>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ReadyAnd")
            .field("inner", &self.inner)
            .finish()
    }
}

// ==== deprecated `Ready` that is just `ReadyAnd` with a unit return type.

/// A future that resolves when a `Service` is ready to accept a request.
///
/// `Ready` values are produced by `ServiceExt::ready`.
pub struct Ready<'a, T, Request>(ReadyAnd<'a, T, Request>);

// Safety: This is safe for the same reason that the impl for ReadyAnd is safe.
impl<'a, T, Request> Unpin for Ready<'a, T, Request> {}

impl<'a, T, Request> Ready<'a, T, Request>
where
    T: Service<Request>,
{
    #[allow(missing_docs)]
    #[deprecated(since = "0.3.1", note = "prefer `ReadyAnd` which yields the service")]
    pub fn new(service: &'a mut T) -> Self {
        Self(ReadyAnd::new(service))
    }
}

impl<'a, T, Request> Future for Ready<'a, T, Request>
where
    T: Service<Request>,
{
    type Output = Result<(), T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let _ = ready!(Pin::new(&mut self.0).poll(cx));
        Poll::Ready(Ok(()))
    }
}

impl<'a, T, Request> fmt::Debug for Ready<'a, T, Request>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Ready").field(&self.0).finish()
    }
}
