//! Future types

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::ready;
use pin_project::{pin_project, project};

use crate::error::{Error, Overloaded};

/// Future for the `LoadShed` service.
#[pin_project]
pub struct ResponseFuture<F> {
    #[pin]
    state: ResponseState<F>,
}

#[pin_project]
enum ResponseState<F> {
    Called(#[pin] F),
    Overloaded,
}

impl<F> ResponseFuture<F> {
    pub(crate) fn called(fut: F) -> Self {
        ResponseFuture {
            state: ResponseState::Called(fut),
        }
    }

    pub(crate) fn overloaded() -> Self {
        ResponseFuture {
            state: ResponseState::Overloaded,
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
        #[project]
        match self.project().state.project() {
            ResponseState::Called(fut) => Poll::Ready(ready!(fut.poll(cx)).map_err(Into::into)),
            ResponseState::Overloaded => Poll::Ready(Err(Overloaded::new().into())),
        }
    }
}

impl<F> fmt::Debug for ResponseFuture<F>
where
    // bounds for future-proofing...
    F: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("ResponseFuture")
    }
}
