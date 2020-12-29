use futures_util::FutureExt;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use futures_util::future::Map as MapResultFuture;

/// Service returned by the [`map_result`] combinator.
///
/// [`map_result`]: crate::util::ServiceExt::map_result
#[derive(Clone, Debug)]
pub struct MapResult<S, F> {
    inner: S,
    f: F,
}

/// A [`Layer`] that produces a [`MapResult`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Debug, Clone)]
pub struct MapResultLayer<F> {
    f: F,
}

impl<S, F> MapResult<S, F> {
    /// Creates a new `MapResult` service.
    pub fn new(inner: S, f: F) -> Self {
        MapResult { f, inner }
    }
}

impl<S, F, Request, Response, Error> Service<Request> for MapResult<S, F>
where
    S: Service<Request>,
    Error: From<S::Error>,
    F: FnOnce(Result<S::Response, S::Error>) -> Result<Response, Error> + Clone,
{
    type Response = Response;
    type Error = Error;
    type Future = MapResultFuture<S::Future, F>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    #[inline]
    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).map(self.f.clone())
    }
}

impl<F> MapResultLayer<F> {
    /// Creates a new [`MapResultLayer`] layer.
    pub fn new(f: F) -> Self {
        MapResultLayer { f }
    }
}

impl<S, F> Layer<S> for MapResultLayer<F>
where
    F: Clone,
{
    type Service = MapResult<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        MapResult {
            f: self.f.clone(),
            inner,
        }
    }
}
