use crate::Error;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pub struct ResponseFuture<F> {
    inner: F,
}

impl<F> Unpin for ResponseFuture<F> {}

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

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { Pin::new_unchecked(&mut self.inner) }
            .poll(cx)
            .map_err(Into::into)
    }
}
