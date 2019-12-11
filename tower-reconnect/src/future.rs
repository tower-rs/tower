use crate::Error;
use pin_project::{pin_project, project};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Future that resolves to the response or failure to connect.
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<F, E> {
    #[pin]
    inner: Inner<F, E>,
}

#[pin_project]
#[derive(Debug)]
enum Inner<F, E> {
    Future(#[pin] F),
    Error(Option<E>),
}

impl<F, E> ResponseFuture<F, E> {
    pub(crate) fn new(inner: F) -> Self {
        ResponseFuture {
            inner: Inner::Future(inner),
        }
    }

    pub(crate) fn error(error: E) -> Self {
        ResponseFuture {
            inner: Inner::Error(Some(error)),
        }
    }
}

impl<F, T, E, ME> Future for ResponseFuture<F, ME>
where
    F: Future<Output = Result<T, E>>,
    E: Into<Error>,
    ME: Into<Error>,
{
    type Output = Result<T, Error>;

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        //self.project().inner.poll(cx).map_err(Into::into)
        let me = self.project();
        #[project]
        match me.inner.project() {
            Inner::Future(fut) => fut.poll(cx).map_err(Into::into),
            Inner::Error(e) => {
                let e = e.take().expect("Polled after ready.").into();
                Poll::Ready(Err(e))
            }
        }
    }
}
