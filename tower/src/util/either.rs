//! Contains [`Either`] and related types and functions.
//!
//! See [`Either`] documentation for more details.

use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;

/// Combine two different service types into a single type.
///
/// Both services must be of the same request, response, and error types.
/// [`Either`] is useful for handling conditional branching in service middleware
/// to different inner service types.
#[derive(Clone, Copy, Debug)]
pub enum Either<A, B> {
    #[allow(missing_docs)]
    A(A),
    #[allow(missing_docs)]
    B(B),
}

impl<A, B, Request> Service<Request> for Either<A, B>
where
    A: Service<Request>,
    B: Service<Request, Response = A::Response, Error = A::Error>,
{
    type Response = A::Response;
    type Error = A::Error;
    type Future = EitherResponseFuture<A::Future, B::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self {
            Either::A(service) => service.poll_ready(cx),
            Either::B(service) => service.poll_ready(cx),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        match self {
            Either::A(service) => EitherResponseFuture {
                kind: Kind::A {
                    inner: service.call(request),
                },
            },
            Either::B(service) => EitherResponseFuture {
                kind: Kind::B {
                    inner: service.call(request),
                },
            },
        }
    }
}

pin_project! {
    /// Response future for [`Either`].
    pub struct EitherResponseFuture<A, B> {
        #[pin]
        kind: Kind<A, B>
    }
}

pin_project! {
    #[project = KindProj]
    enum Kind<A, B> {
        A { #[pin] inner: A },
        B { #[pin] inner: B },
    }
}

impl<A, B> Future for EitherResponseFuture<A, B>
where
    A: Future,
    B: Future<Output = A::Output>,
{
    type Output = A::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().kind.project() {
            KindProj::A { inner } => inner.poll(cx),
            KindProj::B { inner } => inner.poll(cx),
        }
    }
}

impl<S, A, B> Layer<S> for Either<A, B>
where
    A: Layer<S>,
    B: Layer<S>,
{
    type Service = Either<A::Service, B::Service>;

    fn layer(&self, inner: S) -> Self::Service {
        match self {
            Either::A(layer) => Either::A(layer.layer(inner)),
            Either::B(layer) => Either::B(layer.layer(inner)),
        }
    }
}
