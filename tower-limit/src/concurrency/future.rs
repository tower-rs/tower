//! Future types
//!
use super::Error;
use futures_core::ready;
use pin_project::{pin_project, pinned_drop};
use std::sync::Arc;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_sync::semaphore::Semaphore;

/// Future for the `ConcurrencyLimit` service.
#[pin_project(PinnedDrop)]
#[derive(Debug)]
pub struct ResponseFuture<T> {
    #[pin]
    inner: T,
    semaphore: Arc<Semaphore>,
}

impl<T> ResponseFuture<T> {
    pub(crate) fn new(inner: T, semaphore: Arc<Semaphore>) -> ResponseFuture<T> {
        ResponseFuture { inner, semaphore }
    }
}

impl<F, T, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<T, E>>,
    E: Into<Error>,
{
    type Output = Result<T, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(ready!(self.project().inner.poll(cx)).map_err(Into::into))
    }
}

#[pinned_drop]
fn drop_response_future<T>(mut rfut: Pin<&mut ResponseFuture<T>>) {
    rfut.project().semaphore.add_permits(1);
}
