use super::{Pick, Picker};
use crate::util::Either;
use crate::BoxError;
use futures_util::ready;
use pin_project::pin_project;
use std::task::{Context, Poll};
use std::{future::Future, pin::Pin};
use tower_service::Service;

/// TODO(david): docs
#[derive(Debug, Clone, Copy)]
pub struct Multiplex<A, B, P> {
    picker: P,
    first: A,
    first_ready: bool,
    second: B,
    second_ready: bool,
}

impl<A, B, P> Multiplex<A, B, P> {
    /// TODO(david): docs
    pub fn new(first: A, second: B, picker: P) -> Self {
        Self {
            picker,
            first,
            first_ready: false,
            second,
            second_ready: false,
        }
    }
}

impl<R, P, A, B> Service<R> for Multiplex<A, B, P>
where
    P: Picker<R>,
    A: Service<R>,
    A::Error: Into<BoxError>,
    B: Service<R, Response = A::Response>,
    B::Error: Into<BoxError>,
{
    type Response = A::Response;
    type Error = BoxError;
    type Future = ResponseFuture<A::Future, B::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if !self.first_ready {
            ready!(self.first.poll_ready(cx).map_err(Into::into)?);
            self.first_ready = true;
        }

        if !self.second_ready {
            ready!(self.second.poll_ready(cx).map_err(Into::into)?);
            self.second_ready = true;
        }

        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: R) -> Self::Future {
        assert!(
            self.first_ready && self.second_ready,
            "Multiplex must wait for all services to be ready. Did you forget to call poll_ready()?",
        );

        let future = match self.picker.pick(&mut req) {
            Pick::First => {
                self.first_ready = false;
                Either::A(self.first.call(req))
            }
            Pick::Second => {
                self.second_ready = false;
                Either::B(self.second.call(req))
            }
        };

        ResponseFuture { inner: future }
    }
}

/// TODO(david): docs
#[pin_project]
#[derive(Debug)]
pub struct ResponseFuture<AF, BF> {
    #[pin]
    inner: Either<AF, BF>,
}

impl<AFut, AErr, BFut, BErr, Res> Future for ResponseFuture<AFut, BFut>
where
    AFut: Future<Output = Result<Res, AErr>>,
    BFut: Future<Output = Result<Res, BErr>>,
    AErr: Into<BoxError>,
    BErr: Into<BoxError>,
{
    type Output = Result<Res, BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}
