//! Future types

use crate::mock::error::{self, Error};
use std::future::Future;
use std::task::{Poll, Context};
use std::pin::Pin;
use tokio_sync::oneshot;

/// Future of the `Mock` response.
#[derive(Debug)]
pub struct ResponseFuture<T> {
    rx: Option<Rx<T>>,
}

type Rx<T> = oneshot::Receiver<Result<T, Error>>;

impl<T> ResponseFuture<T> {
    pub(crate) fn new(rx: Rx<T>) -> ResponseFuture<T> {
        ResponseFuture { rx: Some(rx) }
    }

    pub(crate) fn closed() -> ResponseFuture<T> {
        ResponseFuture { rx: None }
    }
}

impl<T> Future for ResponseFuture<T> {

    type Output = Result<T, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {

        use futures_util::future::FutureExt;

        match self.rx {
            Some(ref mut rx) => match rx.poll_unpin(cx) {
                Poll::Ready(Ok(Ok(v))) => Poll::Ready(Ok(v.into())),
                Poll::Ready(Ok(Err(e))) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(_)) => Poll::Ready(Err(error::Closed::new().into())),
            },
            None => Poll::Ready(Err(error::Closed::new().into())),
        }
    }
}
