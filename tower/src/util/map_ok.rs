use futures_util::{future::MapOk as MapOkFut, TryFutureExt};
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

/// Service returned by the [`map_ok`] combinator.
///
/// [`map_ok`]: crate::util::ServiceExt::map_ok
#[derive(Debug)]
pub struct MapOk<S, F> {
    inner: S,
    f: F,
}

impl<S, F> MapOk<S, F> {
    /// Creates a new `MapOk` service.
    pub fn new(inner: S, f: F) -> Self {
        MapOk { f, inner }
    }
}

impl<S, F, Request, Response> Service<Request> for MapOk<S, F>
where
    S: Service<Request>,
    F: FnOnce(S::Response) -> Response + Clone,
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

/// A [`Layer`] that produces a [`MapOk`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Debug)]
pub struct MapOkLayer<F> {
    f: F,
}

impl<F> MapOkLayer<F> {
    /// Creates a new [`MapOkLayer`] layer.
    pub fn new(f: F) -> Self {
        MapOkLayer { f }
    }
}

impl<S, F> Layer<S> for MapOkLayer<F>
where
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
