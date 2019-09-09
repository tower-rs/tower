//! Future types

use crate::error::{Elapsed, Error};
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_timer::Delay;

/// `Timeout` response future
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<T> {
    #[pin]
    response: T,
    #[pin]
    sleep: Delay,
}

impl<T> ResponseFuture<T> {
    pub(crate) fn new(response: T, sleep: Delay) -> Self {
        ResponseFuture { response, sleep }
    }
}

impl<F, T, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<T, E>>,
    Error: From<E>,
{
    type Output = Result<T, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // First, try polling the future
        match this.response.poll(cx) {
            Poll::Ready(v) => return Poll::Ready(v.map_err(Error::from)),
            Poll::Pending => {}
        }

        // Now check the sleep
        match this.sleep.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => Poll::Ready(Err(Elapsed(()).into())),
        }
    }
}
