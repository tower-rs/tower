use futures_util::TryFutureExt;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use futures_util::future::MapErr as MapErrFuture;

/// Service returned by the [`map_err`] combinator.
///
/// [`map_err`]: crate::util::ServiceExt::map_err
#[derive(Clone, Debug)]
pub struct MapErr<S, F> {
    inner: S,
    f: F,
}

/// A [`Layer`] that produces a [`MapErr`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Debug)]
pub struct MapErrLayer<F> {
    f: F,
}

impl<S, F> MapErr<S, F> {
    /// Creates a new [`MapErr`] service.
    pub fn new(inner: S, f: F) -> Self {
        MapErr { f, inner }
    }
}

impl<S, F, Request, Error> Service<Request> for MapErr<S, F>
where
    S: Service<Request>,
    F: FnOnce(S::Error) -> Error + Clone,
{
    type Response = S::Response;
    type Error = Error;
    type Future = MapErrFuture<S::Future, F>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(self.f.clone())
    }

    #[inline]
    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).map_err(self.f.clone())
    }
}

impl<F> MapErrLayer<F> {
    /// Creates a new [`MapErrLayer`].
    pub fn new(f: F) -> Self {
        MapErrLayer { f }
    }
}

impl<S, F> Layer<S> for MapErrLayer<F>
where
    F: Clone,
{
    type Service = MapErr<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        MapErr {
            f: self.f.clone(),
            inner,
        }
    }
}
