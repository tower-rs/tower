//! Future types for the `Buffer` middleware.

use super::{error::Closed, message};
use futures_core::ready;
use pin_project::pin_project;
use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Future that completes when the buffered service eventually services the submitted request.
#[pin_project]
pub struct ResponseFuture<S, E2, Response>
where
    S: crate::Service<Response>,
{
    #[pin]
    state: ResponseState<S, E2, Response>,
}

impl<S, E2, Response> Debug for ResponseFuture<S, E2, Response>
where
    S: crate::Service<Response>,
    S::Future: Debug,
    S::Error: Debug,
    E2: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponseFuture")
            .field("state", &self.state)
            .finish()
    }
}

#[pin_project(project = ResponseStateProj)]
enum ResponseState<S, E2, Response>
where
    S: crate::Service<Response>,
{
    Failed(Option<E2>),
    Rx(#[pin] message::Rx<S::Future, S::Error>),
    Poll(#[pin] S::Future),
}

impl<S, E2, Response> Debug for ResponseState<S, E2, Response>
where
    S: crate::Service<Response>,
    S::Future: Debug,
    S::Error: Debug,
    E2: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseState::Failed(e) => f.debug_tuple("ResponseState::Failed").field(e).finish(),
            ResponseState::Rx(rx) => f.debug_tuple("ResponseState::Rx").field(rx).finish(),
            ResponseState::Poll(fut) => f.debug_tuple("ResponseState::Pool").field(fut).finish(),
        }
    }
}

impl<S, E2, Response> ResponseFuture<S, E2, Response>
where
    S: crate::Service<Response>,
{
    pub(crate) fn new(rx: message::Rx<S::Future, S::Error>) -> Self {
        ResponseFuture {
            state: ResponseState::Rx(rx),
        }
    }

    pub(crate) fn failed(err: E2) -> Self {
        ResponseFuture {
            state: ResponseState::Failed(Some(err)),
        }
    }
}

impl<S, E2, Response> Future for ResponseFuture<S, E2, Response>
where
    S: crate::Service<Response>,
    S::Future: Future<Output = Result<S::Response, S::Error>>,
    S::Error: Into<E2>,
    crate::buffer::error::Closed: Into<E2>,
{
    type Output = Result<S::Response, E2>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            match this.state.as_mut().project() {
                ResponseStateProj::Failed(e) => {
                    return Poll::Ready(Err(e.take().expect("polled after error")));
                }
                ResponseStateProj::Rx(rx) => match ready!(rx.poll(cx)) {
                    Ok(Ok(f)) => this.state.set(ResponseState::Poll(f)),
                    Ok(Err(e)) => return Poll::Ready(Err(e.into())),
                    Err(_) => return Poll::Ready(Err(Closed::new().into())),
                },
                ResponseStateProj::Poll(fut) => return fut.poll(cx).map_err(Into::into),
            }
        }
    }
}
