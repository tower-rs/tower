//! Contains `EitherService` and related types and functions.
//!
//! See `EitherService` documentation for more details.

pub mod future;

use self::future::ResponseFuture;
use _either::Either;
use futures::Poll;
use tower_service::Service;

/// Combine two different service types into a single type.
///
/// Both services must be of the same request, response, and error types.
/// `EitherService` is useful for handling conditional branching in service
/// middleware to different inner service types.
pub struct EitherService<A, B> {
    inner: Either<A, B>,
}

type Error = Box<::std::error::Error + Send + Sync>;

impl<A, B> EitherService<A, B> {
    pub fn new<Request>(inner: Either<A, B>) -> EitherService<A, B>
    where
        A: Service<Request>,
        A::Error: Into<Error>,
        B: Service<Request, Response = A::Response>,
        B::Error: Into<Error>,
    {
        EitherService { inner }
    }
}

impl<A, B, Request> Service<Request> for EitherService<A, B>
where
    A: Service<Request>,
    A::Error: Into<Error>,
    B: Service<Request, Response = A::Response>,
    B::Error: Into<Error>,
{
    type Response = A::Response;
    type Error = Error;
    type Future = ResponseFuture<A::Future, B::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        use self::Either::*;

        match &mut self.inner {
            Left(service) => service.poll_ready().map_err(Into::into),
            Right(service) => service.poll_ready().map_err(Into::into),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        use self::Either::*;

        match &mut self.inner {
            Left(service) => ResponseFuture::new_left(service.call(request)),
            Right(service) => ResponseFuture::new_right(service.call(request)),
        }
    }
}
