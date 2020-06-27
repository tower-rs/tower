use futures_util::{future::MapOk as MapOkFut, TryFutureExt};
use std::marker::PhantomData;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

#[derive(Debug)]
pub struct MapOk<S, F> {
    inner: S,
    f: F,
}

impl<S, F> MapOk<S, F> {
    pub fn new(inner: S, f: F) -> Self {
        MapOk { f, inner }
    }
}

impl<S, F, Request, Response> Service<Request> for MapOk<S, F>
where
    S: Service<Request>,
    F: FnOnce(S::Response) -> Response,
    F: Clone,
{
    type Response = Response;
    type Error = S::Error;
    type Future = MapOkFut<S::Future, F>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).map_ok(self.f.clone())
    }
}

#[derive(Debug)]
pub struct MapOkLayer<F, Request, Response> {
    f: F,
    _p: PhantomData<fn(Request, Response)>,
}

impl<F, Request, Error> MapOkLayer<F, Request, Error>
where
    F: Clone,
{
    pub fn new(f: F) -> Self {
        MapOkLayer { f, _p: PhantomData }
    }
}

impl<S, F, Request, Response> Layer<S> for MapOkLayer<F, Request, Response>
where
    S: Service<Request>,
    F: FnOnce(S::Response) -> Response,
    F: Clone,
{
    type Service = MapOk<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        MapOk {
            f: self.f.clone(),
            inner,
        }
    }
}
