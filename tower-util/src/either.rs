//! Contains `EitherService` and related types and functions.
//!
//! See `EitherService` documentation for more details.

use futures::future::Either;
use futures::Poll;
use tower_service::Service;

/// Combine two different service types into a single type.
///
/// Both services must be of the same request, response, and error types.
/// `EitherService` is useful for handling conditional branching in service
/// middleware to different inner service types.
pub enum EitherService<A, B> {
    A(A),
    B(B),
}

impl<A, B, Request> Service<Request> for EitherService<A, B>
where
    A: Service<Request>,
    B: Service<Request, Response = A::Response, Error = A::Error>,
{
    type Response = A::Response;
    type Error = A::Error;
    type Future = Either<A::Future, B::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        use self::EitherService::*;

        match *self {
            A(ref mut service) => service.poll_ready(),
            B(ref mut service) => service.poll_ready(),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        use self::EitherService::*;

        match *self {
            A(ref mut service) => Either::A(service.call(request)),
            B(ref mut service) => Either::B(service.call(request)),
        }
    }
}
