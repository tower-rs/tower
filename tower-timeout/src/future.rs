//! Future types

use crate::error::{Elapsed, Error};
use futures::{Async, Future, Poll};
use tokio_timer::Delay;

/// `Timeout` response future
#[derive(Debug)]
pub struct ResponseFuture<T> {
    response: T,
    sleep: Delay,
}

impl<T> ResponseFuture<T> {
    pub(crate) fn new(response: T, sleep: Delay) -> Self {
        ResponseFuture { response, sleep }
    }
}

impl<T> Future for ResponseFuture<T>
where
    T: Future,
    Error: From<T::Error>,
{
    type Item = T::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // First, try polling the future
        match self.response.poll()? {
            Async::Ready(v) => return Ok(Async::Ready(v)),
            Async::NotReady => {}
        }

        // Now check the sleep
        match self.sleep.poll()? {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(_) => Err(Elapsed(()).into()),
        }
    }
}
