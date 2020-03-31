//! Future types
//!
use super::sync::semaphore::Semaphore;
use futures_core::ready;
use pin_project::{pin_project, pinned_drop};
use std::sync::Arc;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

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
{
    type Output = Result<T, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(ready!(self.project().inner.poll(cx)))
    }
}

#[pinned_drop]
impl<T> PinnedDrop for ResponseFuture<T> {
    fn drop(self: Pin<&mut Self>) {
        self.project().semaphore.add_permits(1);
    }
}
