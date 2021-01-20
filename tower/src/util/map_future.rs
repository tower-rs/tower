use std::{
    future::Future,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;

pub struct MapFuture<S, F> {
    inner: S,
    f: F,
}

#[derive(Debug, Clone)]
pub struct MapFutureLayer<F> {
    f: F,
}

impl<S, F> MapFuture<S, F> {
    pub fn new(inner: S, f: F) -> Self {
        Self { inner, f }
    }

    pub fn layer(f: F) -> MapFutureLayer<F> {
        MapFutureLayer { f }
    }
}

impl<R, S, F, T, E, Fut> Service<R> for MapFuture<S, F>
where
    S: Service<R>,
    F: FnMut(S::Future) -> Fut,
    E: From<S::Error>,
    Fut: Future<Output = Result<T, E>>,
{
    type Response = T;
    type Error = E;
    type Future = Fut;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(From::from)
    }

    fn call(&mut self, req: R) -> Self::Future {
        (self.f)(self.inner.call(req))
    }
}

impl<S, F> Layer<S> for MapFuture<S, F>
where
    F: Clone,
{
    type Service = MapFuture<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        MapFuture::new(inner, self.f.clone())
    }
}
