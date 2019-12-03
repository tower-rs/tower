use crate::Error;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Future that resolves to the response or failure to connect.
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<F> {
    #[pin]
    inner: F,
}

impl<F> ResponseFuture<F> {
    pub(crate) fn new(inner: F) -> Self {
        ResponseFuture { inner }
    }
}

impl<F, T, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<T, E>>,
    E: Into<Error>,
{
    type Output = Result<T, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().inner.poll(cx).map_err(Into::into)
    }
}
