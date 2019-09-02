//! Future types

use crate::mock::error::{self, Error};
use tokio_sync::oneshot;

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Future of the `Mock` response.
#[derive(Debug)]
pub struct ResponseFuture<T> {
    rx: Option<Rx<T>>,
}

type Rx<T> = oneshot::Receiver<Result<T, Error>>;

impl<T> ResponseFuture<T> {
    pin_utils::unsafe_pinned!(rx: Option<Rx<T>>);

    pub(crate) fn new(rx: Rx<T>) -> ResponseFuture<T> {
        ResponseFuture { rx: Some(rx) }
    }

    pub(crate) fn closed() -> ResponseFuture<T> {
        ResponseFuture { rx: None }
    }
}

impl<T> Future for ResponseFuture<T> {
    type Output = Result<T, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match self.rx().as_pin_mut() {
            Some(rx) => match rx.poll(cx) {
                Poll::Ready(Ok(Ok(v))) => Poll::Ready(Ok(v)),
                Poll::Ready(Ok(Err(e))) => Poll::Ready(Err(e)),
                Poll::Ready(Err(_)) => Poll::Ready(Err(error::Closed::new().into())),
                Poll::Pending => Poll::Pending,
            },
            None => Poll::Ready(Err(error::Closed::new().into())),
        }
    }
}
