use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

/// A [`tower::Service`] which maps from one request type to another.
///
/// [`tower::Service`]: ../trait.Service.html
#[derive(Debug, Clone)]
pub struct MapRequest<S, F> {
    inner: S,
    f: F,
}

impl<S, F> MapRequest<S, F> {
    /// Create a new `Map` service.
    pub fn new(inner: S, f: F) -> Self {
        MapRequest { inner, f }
    }
}

impl<S, F, R1, R2> Service<R1> for MapRequest<S, F>
where
    S: Service<R2>,
    F: FnMut(R1) -> R2,
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

/// A [`tower::Layer`] that accepts an [`FnMut`] and applies it
/// to each inner service. Note that [`FnMut`] will implement
/// [`Clone`] _if_ all the captured values implement either [`Copy`]
/// or [`Clone`].
///
/// [`tower::Layer`]: ../trait.Layer.html
/// [`FnMut`]: https://doc.rust-lang.org/std/ops/trait.FnMut.html
/// [`Clone`]: https://doc.rust-lang.org/std/clone/trait.Clone.html
/// [`Copy`]: https://doc.rust-lang.org/std/marker/trait.Copy.html
#[derive(Debug, Clone)]
pub struct MapRequestLayer<F> {
    f: F,
}

impl<F> MapRequestLayer<F> {
    /// Create a `MapLayer` from some `Fn`.
    pub fn new(f: F) -> Self {
        MapRequestLayer { f }
    }
}

impl<F, S> Layer<S> for MapRequestLayer<F>
where
    F: Clone,
{
    type Service = MapRequest<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        MapRequest::new(inner, self.f.clone())
    }
}
