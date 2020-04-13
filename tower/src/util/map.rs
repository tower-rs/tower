use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

/// A service to map from one request type to another.
#[derive(Debug, Clone)]
pub struct Map<S, F> {
    inner: S,
    f: F,
}

impl<S, F> Map<S, F> {
    /// Create a new `Map` service.
    pub fn new(inner: S, f: F) -> Self {
        Map { inner, f }
    }
}

impl<S, F, R1, R2> Service<R1> for Map<S, F>
where
    S: Service<R2>,
    F: Fn(R1) -> R2,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: R1) -> Self::Future {
        let req = (self.f)(req);
        self.inner.call(req)
    }
}

/// A map layer that takes some clone `FnMut` and will apply it
/// to each inner service.
#[derive(Debug, Clone)]
pub struct MapLayer<F> {
    f: F,
}

impl<F> MapLayer<F> {
    /// Create a `MapLayer` from some `Fn`.
    pub fn new(f: F) -> Self {
        MapLayer { f }
    }
}

impl<F, S> Layer<S> for MapLayer<F>
where
    F: Clone,
{
    type Service = Map<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        Map::new(inner, self.f.clone())
    }
}
