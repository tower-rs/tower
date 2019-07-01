use std::{fmt, marker::PhantomData};

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
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

impl<T: Unpin, Request> Unpin for Ready<T, Request> {}
impl<T, Request> Future for Ready<T, Request>
where
    T: Service<Request> + Unpin,
{
    type Output = Result<T, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<T, T::Error>> {
        match self.inner {
            Some(ref mut service) => {
                match service.poll_ready(cx) {
                    Poll::Ready(Ok(_)) => {}
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                };
            }
            None => panic!("called `poll` after future completed"),
        }

        Ok(self.inner.take().unwrap()).into()
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
