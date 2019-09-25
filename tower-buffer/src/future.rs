//! Future types

use crate::{
    error::{Closed, Error},
    message,
};
use futures_core::ready;
use pin_project::{pin_project, project};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Future eventually completed with the response to the original request.
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<T> {
    #[pin]
    state: ResponseState<T>,
}

#[pin_project]
#[derive(Debug)]
enum ResponseState<T> {
    Failed(Option<Error>),
    Rx(#[pin] message::Rx<T>),
    Poll(#[pin] T),
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

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            #[project]
            match this.state.as_mut().project() {
                ResponseState::Failed(e) => {
                    return Poll::Ready(Err(e.take().expect("polled after error")));
                }
                ResponseState::Rx(rx) => match ready!(rx.poll(cx)) {
                    Ok(Ok(f)) => this.state.set(ResponseState::Poll(f)),
                    Ok(Err(e)) => return Poll::Ready(Err(e.into())),
                    Err(_) => return Poll::Ready(Err(Closed::new().into())),
                },
                ResponseState::Poll(fut) => return fut.poll(cx).map_err(Into::into),
            }
        }
    }
}
