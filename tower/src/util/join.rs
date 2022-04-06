use crate::BoxError;
use futures_util::future::{self, MapErr, TryFuture, TryFutureExt, TryJoin};
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower_service::Service;

#[derive(Debug)]
pub struct Join<A, B> {
    a: A,
    a_ready: bool,
    b: B,
    b_ready: bool,
}

impl<A, B> Join<A, B> {
    pub fn new(a: A, b: B) -> Self {
        Self {
            a,
            b,
            a_ready: false,
            b_ready: false,
        }
    }
}

impl<A, B, Request> Service<Request> for Join<A, B>
where
    Request: Clone,
    A: Service<Request>,
    B: Service<Request>,
    BoxError: From<A::Error> + From<B::Error>,
{
    type Response = (A::Response, B::Response);
    type Error = BoxError;
    type Future = JoinFuture<A::Future, B::Future>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if !self.a_ready {
            match self.a.poll_ready(cx) {
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
                Poll::Ready(Ok(())) => self.a_ready = true,
                Poll::Pending => {}
            }
        }

        if !self.b_ready {
            match self.b.poll_ready(cx) {
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
                Poll::Ready(Ok(())) => self.b_ready = true,
                Poll::Pending => {}
            }
        }

        if self.a_ready && self.b_ready {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    #[inline]
    fn call(&mut self, request: Request) -> Self::Future {
        let a = self
            .a
            .call(request.clone())
            .map_err(Into::into as fn(_) -> _);
        let b = self.b.call(request).map_err(Into::into as fn(_) -> _);
        JoinFuture {
            inner: future::try_join(a, b),
        }
    }
}

pin_project! {
    pub struct JoinFuture<A: TryFuture, B: TryFuture> {
        #[pin]
        inner: TryJoin<MapErr<A, fn(A::Error) -> BoxError>, MapErr<B, fn(B::Error) -> BoxError>>,
    }
}

impl<A, B> Future for JoinFuture<A, B>
where
    A: TryFuture,
    BoxError: From<A::Error>,
    B: TryFuture,
    BoxError: From<B::Error>,
{
    type Output = Result<(A::Ok, B::Ok), BoxError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}
