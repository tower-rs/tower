//! Future types

use super::error::Error;
use futures_core::ready;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// Filtered response future
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<T, S, Request>
where
    S: Service<Request>,
{
    #[pin]
    /// Response future state
    state: State<Request, S::Future>,

    #[pin]
    /// Predicate future
    check: T,

    /// Inner service
    service: S,
}

#[pin_project(project = StateProj)]
#[derive(Debug)]
enum State<Request, U> {
    Check(Option<Request>),
    WaitResponse(#[pin] U),
}

impl<F, T, S, Request> ResponseFuture<F, S, Request>
where
    F: Future<Output = Result<T, Error>>,
    S: Service<Request>,
    S::Error: Into<crate::BoxError>,
{
    pub(crate) fn new(request: Request, check: F, service: S) -> Self {
        ResponseFuture {
            state: State::Check(Some(request)),
            check,
            service,
        }
    }
}

impl<F, T, S, Request> Future for ResponseFuture<F, S, Request>
where
    F: Future<Output = Result<T, Error>>,
    S: Service<Request>,
    S::Error: Into<crate::BoxError>,
{
    type Output = Result<S::Response, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            match this.state.as_mut().project() {
                StateProj::Check(request) => {
                    let request = request
                        .take()
                        .expect("we either give it back or leave State::Check once we take");

                    // Poll predicate
                    match this.check.as_mut().poll(cx)? {
                        Poll::Ready(_) => {
                            let response = this.service.call(request);
                            this.state.set(State::WaitResponse(response));
                        }
                        Poll::Pending => {
                            this.state.set(State::Check(Some(request)));
                            return Poll::Pending;
                        }
                    }
                }
                StateProj::WaitResponse(response) => {
                    return Poll::Ready(ready!(response.poll(cx)).map_err(Error::inner));
                }
            }
        }
    }
}
