//! Future types

use crate::{error::Error, message};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future eventually completed with the response to the original request.
pub struct ResponseFuture<T> {
    state: ResponseState<T>,
}
impl<T> Unpin for ResponseFuture<T> {}

enum ResponseState<T> {
    Failed(Option<Error>),
    Rx(message::Rx<T>),
    Poll(T),
}

impl<T, A, B> ResponseFuture<T>
where
    T: Future<Output = Result<A, B>>,
    B: Into<Error>,
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

impl<T, A, B> Future for ResponseFuture<T>
where
    T: Future<Output = Result<A, B>>,
    B: Into<Error>,
{
    type Output = Result<A, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let fut;

            match self.state {
                ResponseState::Failed(ref mut e) => {
                    return Poll::Ready(Err(e.take().expect("polled after error")));
                }
                ResponseState::Rx(ref mut rx) => match unsafe { Pin::new_unchecked(rx) }.poll(cx) {
                    Poll::Ready(Ok(Ok(f))) => fut = f,
                    Poll::Ready(Ok(Err(e))) => return Poll::Ready(Err(e.into())),
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
                    Poll::Pending => return Poll::Pending,
                    // Err(_) => return Err(Closed::new().into()),
                },
                ResponseState::Poll(ref mut fut) => {
                    return unsafe { Pin::new_unchecked(fut) }
                        .poll(cx)
                        .map_err(Into::into);
                }
            }

            self.state = ResponseState::Poll(fut);
        }
    }
}
