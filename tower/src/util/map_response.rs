use futures_util::TryFutureExt;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

pub use futures_util::future::MapOk as MapResponseFuture;

/// Service returned by the [`map_response`] combinator.
///
/// [`map_response`]: crate::util::ServiceExt::map_response
#[derive(Clone, Debug)]
pub struct MapResponse<S, F> {
    inner: S,
    f: F,
}

/// A [`Layer`] that produces a [`MapResponse`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Debug, Clone)]
pub struct MapResponseLayer<F> {
    f: F,
}

impl<S, F> MapResponse<S, F> {
    /// Creates a new `MapResponse` service.
    pub fn new(inner: S, f: F) -> Self {
        MapResponse { f, inner }
    }
}

impl<S, F, Request, Response> Service<Request> for MapResponse<S, F>
where
    S: Service<Request>,
    F: FnOnce(S::Response) -> Response + Clone,
{
    type Response = Response;
    type Error = S::Error;
    type Future = MapResponseFuture<S::Future, F>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    #[inline]
    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).map_ok(self.f.clone())
    }
}

impl<F> MapResponseLayer<F> {
    /// Creates a new [`MapResponseLayer`] layer.
    pub fn new(f: F) -> Self {
        MapResponseLayer { f }
    }
}

impl<S, F> Layer<S> for MapResponseLayer<F>
where
    F: Clone,
{
    type Service = MapResponse<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        MapResponse {
            f: self.f.clone(),
            inner,
        }
    }
}
