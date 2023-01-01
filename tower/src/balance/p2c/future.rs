//! Future types

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::ready;
use pin_project_lite::pin_project;

use crate::timeout::error::Elapsed;

pin_project! {
    /// Future for the [`Balance`] service.
    ///
    /// [`Balance`]: crate::balance::p2c::Balance
    #[derive(Debug)]
    pub struct ResponseFuture<F> {
        #[pin]
        state: ResponseState<F>,
    }
}

pin_project! {
    #[project = ResponseStateProj]
    #[derive(Debug)]
    enum ResponseState<F> {
        Called {
            #[pin]
            fut: F
        },
        TimedOut,
    }
}

impl<F> ResponseFuture<F> {
    pub(crate) fn called(fut: F) -> Self {
        ResponseFuture {
            state: ResponseState::Called { fut },
        }
    }

    pub(crate) fn timed_out() -> Self {
        ResponseFuture {
            state: ResponseState::TimedOut,
        }
    }
}

impl<F, T, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<T, E>>,
    E: Into<crate::BoxError>,
{
    type Output = Result<T, crate::BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().state.project() {
            ResponseStateProj::Called { fut } => {
                Poll::Ready(ready!(fut.poll(cx)).map_err(Into::into))
            }
            ResponseStateProj::TimedOut => Poll::Ready(Err(Elapsed::new().into())),
        }
    }
}
