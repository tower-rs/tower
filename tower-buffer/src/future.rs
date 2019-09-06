//! Future types

use crate::{
    error::{Closed, Error},
    message,
};
use futures_core::ready;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Future eventually completed with the response to the original request.
pub struct ResponseFuture<T> {
    state: ResponseState<T>,
}

enum ResponseState<T> {
    Failed(Option<Error>),
    Rx(message::Rx<T>),
    Poll(T),
}

impl<T> ResponseFuture<T> {
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

impl<F, T, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<T, E>>,
    E: Into<Error>,
{
    type Output = Result<T, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // we only ever give out Pin to what's inside state when it is ResponseState::Poll
        // we only give it out once state has been set to ResponseState::Poll
        // once state has been set to ResponseState::Poll, we never move it again
        let this = unsafe { self.get_unchecked_mut() };

        loop {
            match this.state {
                ResponseState::Failed(ref mut e) => {
                    return Poll::Ready(Err(e.take().expect("polled after error")));
                }
                ResponseState::Rx(ref mut rx) => match ready!(Pin::new(rx).poll(cx)) {
                    Ok(Ok(f)) => this.state = ResponseState::Poll(f),
                    Ok(Err(e)) => return Poll::Ready(Err(e.into())),
                    Err(_) => return Poll::Ready(Err(Closed::new().into())),
                },
                ResponseState::Poll(ref mut fut) => {
                    let fut = unsafe { Pin::new_unchecked(fut) };
                    return fut.poll(cx).map_err(Into::into);
                }
            }
        }
    }
}
