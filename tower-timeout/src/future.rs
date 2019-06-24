//! Future types

use crate::error::{Elapsed, Error};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_timer::Delay;

/// `Timeout` response future
#[derive(Debug)]
pub struct ResponseFuture<T, R, E> {
    response: T,
    sleep: Delay,
    _pd: std::marker::PhantomData<(R, E)>,
}

impl<T, R, E> ResponseFuture<T, R, E> {
    pub(crate) fn new(response: T, sleep: Delay) -> Self {
        ResponseFuture {
            response,
            sleep,
            _pd: std::marker::PhantomData,
        }
    }
}

impl<T, R, E> Unpin for ResponseFuture<T, R, E> {}
impl<T, R, E> Future for ResponseFuture<T, R, E>
where
    T: Future<Output = Result<R, E>> + Unpin,
    Error: From<E>,
{
    type Output = Result<R, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;

        // First, try polling the future
        match Pin::new(&mut me.response).poll(cx)? {
            Poll::Ready(v) => return Poll::Ready(Ok(v)),
            Poll::Pending => {}
        }

        // Now check the sleep
        match Pin::new(&mut me.sleep).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => Poll::Ready(Err(Elapsed(()).into())),
        }
    }
}
