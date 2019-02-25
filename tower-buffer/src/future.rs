//! Future types

use error::{Error, ServiceError};
use futures::{Async, Future, Poll};
use message;
use std::sync::Arc;

/// Future eventually completed with the response to the original request.
pub struct ResponseFuture<T, E> {
    state: ResponseState<T, E>,
}

enum ResponseState<T, E> {
    Full,
    Failed(Arc<ServiceError<E>>),
    Rx(message::Rx<T, E>),
    Poll(T),
}

impl<T> ResponseFuture<T, T::Error>
where
    T: Future,
{
    pub(crate) fn new(rx: message::Rx<T, T::Error>) -> Self {
        ResponseFuture {
            state: ResponseState::Rx(rx),
        }
    }

    pub(crate) fn failed(err: Arc<ServiceError<T::Error>>) -> Self {
        ResponseFuture {
            state: ResponseState::Failed(err),
        }
    }

    pub(crate) fn full() -> Self {
        ResponseFuture {
            state: ResponseState::Full,
        }
    }
}

impl<T> Future for ResponseFuture<T, T::Error>
where
    T: Future,
{
    type Item = T::Item;
    type Error = Error<T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::ResponseState::*;

        loop {
            let fut;

            match self.state {
                Full => {
                    return Err(Error::Full);
                }
                Failed(ref e) => {
                    return Err(Error::Closed(e.clone()));
                }
                Rx(ref mut rx) => match rx.poll() {
                    Ok(Async::Ready(Ok(f))) => fut = f,
                    Ok(Async::Ready(Err(e))) => return Err(Error::Closed(e)),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(_) => unreachable!(
                        "Worker exited without sending error to all outstanding requests."
                    ),
                },
                Poll(ref mut fut) => {
                    return fut.poll().map_err(Error::Inner);
                }
            }

            self.state = Poll(fut);
        }
    }
}
