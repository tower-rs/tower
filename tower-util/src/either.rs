//! Contains `Either` and related types and functions.
//!
//! See `Either` documentation for more details.

use futures::{Future, Poll};
use tower_service::Service;

/// Combine two different service types into a single type.
///
/// Both services must be of the same request, response, and error types.
/// `Either` is useful for handling conditional branching in service middleware
/// to different inner service types.
#[derive(Clone, Debug)]
pub enum Either<A, B> {
    A(A),
    B(B),
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

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        use self::Either::*;

        match self {
            A(service) => service.poll_ready().map_err(Into::into),
            B(service) => service.poll_ready().map_err(Into::into),
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

impl<A, B> Future for Either<A, B>
where
    A: Future,
    A::Error: Into<Error>,
    B: Future<Item = A::Item>,
    B::Error: Into<Error>,
{
    type Item = A::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::Either::*;

        match self {
            A(fut) => fut.poll().map_err(Into::into),
            B(fut) => fut.poll().map_err(Into::into),
        }
    }
}
