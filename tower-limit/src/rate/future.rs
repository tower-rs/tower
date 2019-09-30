//! Future types

use super::error::Error;
use futures_core::ready;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Future for the `RateLimit` service.
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<T> {
    #[pin]
    inner: T,
}

impl<T> ResponseFuture<T> {
    pub(crate) fn new(inner: T) -> ResponseFuture<T> {
        ResponseFuture { inner }
    }
}

impl<F, T, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<T, E>>,
    Error: From<E>,
{
    type Output = Result<T, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(Ok(ready!(self.project().inner.poll(cx))?))
    }
}
