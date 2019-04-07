//! Future types

use crate::error::{Closed, Error};
use futures::{Async, Future, Poll};
use crate::message;

/// Future eventually completed with the response to the original request.
pub struct ResponseFuture<T> {
    state: ResponseState<T>,
}

enum ResponseState<T> {
    Failed(Option<Error>),
    Rx(message::Rx<T>),
    Poll(T),
}

impl<T> ResponseFuture<T>
where
    T: Future,
    T::Error: Into<Error>,
{
    pub(crate) fn new(rx: message::Rx<T>) -> Self {
        ResponseFuture {
            state: ResponseState::Rx(rx),
        }
    }

    pub(crate) fn failed(err: Error) -> Self {
        ResponseFuture {
            state: ResponseState::Failed(Some(err)),
        }
    }
}

impl<T> Future for ResponseFuture<T>
where
    T: Future,
    T::Error: Into<Error>,
{
    type Item = T::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::ResponseState::*;

        loop {
            let fut;

            match self.state {
                Failed(ref mut e) => {
                    return Err(e.take().expect("polled after error"));
                }
                Rx(ref mut rx) => match rx.poll() {
                    Ok(Async::Ready(Ok(f))) => fut = f,
                    Ok(Async::Ready(Err(e))) => return Err(e.into()),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(_) => return Err(Closed::new().into()),
                },
                Poll(ref mut fut) => {
                    return fut.poll().map_err(Into::into);
                }
            }

            self.state = Poll(fut);
        }
    }
}
