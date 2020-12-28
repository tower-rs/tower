use futures_util::future::{ready, Either, Ready};
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

/// Service returned by the [`try_map_request`] combinator.
///
/// [`try_map_request`]: crate::util::ServiceExt::try_map_request
#[derive(Clone, Debug)]
pub struct TryMapRequest<S, F> {
    inner: S,
    f: F,
}

/// A [`Layer`] that produces a [`TryMapRequest`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Debug)]
pub struct TryMapRequestLayer<F> {
    f: F,
}

impl<S, F> TryMapRequest<S, F> {
    /// Creates a new [`TryMapRequest`] service.
    pub fn new(inner: S, f: F) -> Self {
        TryMapRequest { inner, f }
    }
}

impl<S, F, R1, R2> Service<R1> for TryMapRequest<S, F>
where
    S: Service<R2>,
    F: FnMut(R1) -> Result<R2, S::Error>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Either<S::Future, Ready<Result<S::Response, S::Error>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: R1) -> Self::Future {
        match (self.f)(request) {
            Ok(ok) => Either::Left(self.inner.call(ok)),
            Err(err) => Either::Right(ready(Err(err))),
        }
    }
}

impl<F> TryMapRequestLayer<F> {
    /// Creates a new [`TryMapRequestLayer`].
    pub fn new(f: F) -> Self {
        TryMapRequestLayer { f }
    }
}

impl<S, F> Layer<S> for TryMapRequestLayer<F>
where
    F: Clone,
{
    type Service = TryMapRequest<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        TryMapRequest {
            f: self.f.clone(),
            inner,
        }
    }
}
