use crate::util::Either;
use crate::BoxError;
use futures_util::ready;
use pin_project::pin_project;
use std::task::{Context, Poll};
use std::{future::Future, pin::Pin};
use tower_layer::Layer;
use tower_service::Service;

#[derive(Debug, Clone, Copy)]
pub struct MultiplexLayer<A, P> {
    first: A,
    picker: P,
}

impl<A, P> MultiplexLayer<A, P> {
    pub fn new(first: A, picker: P) -> Self {
        Self { picker, first }
    }
}

impl<P, A, B> Layer<B> for MultiplexLayer<A, P>
where
    P: Clone,
    A: Clone,
{
    type Service = Multiplex<A, B, P>;

    fn layer(&self, inner: B) -> Self::Service {
        Multiplex::new(self.first.clone(), inner, self.picker.clone())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Multiplex<A, B, P> {
    picker: P,
    first: A,
    first_ready: bool,
    second: B,
    second_ready: bool,
}

impl<A, B, P> Multiplex<A, B, P> {
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

pub trait Picker<R: ?Sized> {
    fn pick(&mut self, req: &mut R) -> Pick;
}

#[derive(Debug, Copy, Clone)]
pub enum Pick {
    First,
    Second,
}

impl<F, T> Picker<T> for F
where
    F: FnMut(&mut T) -> Pick,
{
    fn pick(&mut self, req: &mut T) -> Pick {
        self(req)
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

        self.first_ready = false;
        self.second_ready = false;

        let future = match self.picker.pick(&mut req) {
            Pick::First => Either::A(self.first.call(req)),
            Pick::Second => Either::B(self.second.call(req)),
        };

        ResponseFuture { inner: future }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{service_fn, Service, ServiceBuilder, ServiceExt};

    #[tokio::test]
    async fn basic() {
        let mut svc = ServiceBuilder::new()
            .multiplex(service_fn(one), pick_if(1))
            .multiplex(service_fn(two), pick_if(2))
            .multiplex(service_fn(three), pick_if(3))
            .service(service_fn(bottom));

        assert_eq!(svc.ready_and().await.unwrap().call(1).await.unwrap(), "one",);

        assert_eq!(svc.ready_and().await.unwrap().call(2).await.unwrap(), "two",);

        assert_eq!(
            svc.ready_and().await.unwrap().call(3).await.unwrap(),
            "three",
        );

        assert_eq!(
            svc.ready_and().await.unwrap().call(4).await.unwrap(),
            "bottom",
        );
    }

    fn pick_if(n: i32) -> impl Picker<i32> + Clone {
        move |req: &mut i32| {
            if n == *req {
                Pick::First
            } else {
                Pick::Second
            }
        }
    }

    async fn one(req: i32) -> Result<&'static str, BoxError> {
        assert_eq!(req, 1);
        Ok("one")
    }

    async fn two(req: i32) -> Result<&'static str, BoxError> {
        assert_eq!(req, 2);
        Ok("two")
    }

    async fn three(req: i32) -> Result<&'static str, BoxError> {
        assert_eq!(req, 3);
        Ok("three")
    }

    async fn bottom(_req: i32) -> Result<&'static str, BoxError> {
        Ok("bottom")
    }
}
