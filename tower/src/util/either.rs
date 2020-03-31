//! Contains `Either` and related types and functions.
//!
//! See `Either` documentation for more details.

use futures_core::ready;
use pin_project::{pin_project, project};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// Combine two different service types into a single type.
///
/// Both services must be of the same request, response, and error types.
/// `Either` is useful for handling conditional branching in service middleware
/// to different inner service types.
#[pin_project]
#[derive(Clone, Debug)]
pub enum Either<A, B> {
    /// One type of backing `Service`.
    A(#[pin] A),
    /// The other type of backing `Service`.
    B(#[pin] B),
}

type Error = Box<dyn std::error::Error + Send + Sync>;

impl<A, B, Request> Service<Request> for Either<A, B>
where
    A: Service<Request>,
    A::Error: Into<Error>,
    B: Service<Request, Response = A::Response>,
    B::Error: Into<Error>,
{
    type Response = A::Response;
    type Error = Error;
    type Future = Either<A::Future, B::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        use self::Either::*;

        match self {
            A(service) => Poll::Ready(Ok(ready!(service.poll_ready(cx)).map_err(Into::into)?)),
            B(service) => Poll::Ready(Ok(ready!(service.poll_ready(cx)).map_err(Into::into)?)),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        use self::Either::*;

        match self {
            A(service) => A(service.call(request)),
            B(service) => B(service.call(request)),
        }
    }
}

impl<A, B, T, AE, BE> Future for Either<A, B>
where
    A: Future<Output = Result<T, AE>>,
    AE: Into<Error>,
    B: Future<Output = Result<T, BE>>,
    BE: Into<Error>,
{
    type Output = Result<T, Error>;

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        #[project]
        match self.project() {
            Either::A(fut) => Poll::Ready(Ok(ready!(fut.poll(cx)).map_err(Into::into)?)),
            Either::B(fut) => Poll::Ready(Ok(ready!(fut.poll(cx)).map_err(Into::into)?)),
        }
    }
}
