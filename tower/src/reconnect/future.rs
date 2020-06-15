use pin_project::pin_project;
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

#[pin_project(project = InnerProj)]
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
    E: Into<crate::BoxError>,
    ME: Into<crate::BoxError>,
{
    type Output = Result<T, crate::BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();
        match me.inner.project() {
            InnerProj::Future(fut) => fut.poll(cx).map_err(Into::into),
            InnerProj::Error(e) => {
                let e = e.take().expect("Polled after ready.").into();
                Poll::Ready(Err(e))
            }
        }
    }
}
