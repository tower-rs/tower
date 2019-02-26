//! Future types

use error::{Error, ServiceError};
use futures::{Async, Future, Poll};
use message;

/// Future eventually completed with the response to the original request.
pub struct ResponseFuture<T> {
    state: ResponseState<T>,
}

enum ResponseState<T> {
    Failed(ServiceError),
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

    pub(crate) fn failed(err: ServiceError) -> Self {
        ResponseFuture {
            state: ResponseState::Failed(err),
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
                Failed(ref e) => {
                    return Err(e.clone().into());
                }
                Rx(ref mut rx) => match rx.poll() {
                    Ok(Async::Ready(Ok(f))) => fut = f,
                    Ok(Async::Ready(Err(e))) => return Err(e.into()),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(_) => unreachable!(
                        "Worker exited without sending error to all outstanding requests."
                    ),
                },
                Poll(ref mut fut) => {
                    return fut.poll().map_err(Into::into);
                }
            }

            self.state = Poll(fut);
        }
    }
}
