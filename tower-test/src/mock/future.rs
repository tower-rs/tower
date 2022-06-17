//! Future types

use crate::mock::error::{self, Error};
use futures_util::ready;
use pin_project_lite::pin_project;
use tokio::sync::oneshot;

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

pin_project! {
    /// Future of the `Mock` response.
    #[derive(Debug)]
    pub struct ResponseFuture<T> {
        #[pin]
        rx: Option<Rx<T>>,
    }
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match self.project().rx.as_pin_mut() {
            Some(rx) => match ready!(rx.poll(cx)) {
                Ok(r) => Poll::Ready(r),
                Err(_) => Poll::Ready(Err(error::Closed::new().into())),
            },
            None => Poll::Ready(Err(error::Closed::new().into())),
        }
    }
}
