//! Future types

use crate::mock::error::{self, Error};
use futures::{Async, Future, Poll};
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
    type Item = T;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.rx {
            Some(ref mut rx) => match rx.poll() {
                Ok(Async::Ready(Ok(v))) => Ok(v.into()),
                Ok(Async::Ready(Err(e))) => Err(e),
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(_) => Err(error::Closed::new().into()),
            },
            None => Err(error::Closed::new().into()),
        }
    }
}
