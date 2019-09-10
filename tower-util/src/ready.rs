use std::{fmt, marker::PhantomData};

use futures_util::ready;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// Future yielding a `Service` once the service is ready to process a request
///
/// `Ready` values are produced by `ServiceExt::ready`.
#[pin_project]
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
    type Output = Result<T, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        ready!(this
            .inner
            .as_mut()
            .expect("called `poll` after future completed")
            .poll_ready(cx))?;

        Poll::Ready(Ok(this.inner.take().unwrap()))
    }
}

impl<T, Request> fmt::Debug for Ready<T, Request>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Ready").field("inner", &self.inner).finish()
    }
}
