use super::{error, Error};
use futures_core::ready;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Response future returned by `Optional`.
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<T> {
    #[pin]
    inner: Option<T>,
}

impl<T> ResponseFuture<T> {
    pub(crate) fn new(inner: Option<T>) -> ResponseFuture<T> {
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
        match self.project().inner.as_pin_mut() {
            Some(inner) => Poll::Ready(Ok(ready!(inner.poll(cx)).map_err(Into::into)?)),
            None => Poll::Ready(Err(error::None::new().into())),
        }
    }
}
