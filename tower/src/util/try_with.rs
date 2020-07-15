use futures_util::future::{ready, Either, Ready};
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

/// Service returned by the [`try_with`] combinator.
///
/// [`try_with`]: crate::util::ServiceExt::try_with
#[derive(Debug)]
pub struct TryWith<S, F> {
    inner: S,
    f: F,
}

impl<S, F> TryWith<S, F> {
    /// Creates a new [`TryWith`] service.
    pub fn new(inner: S, f: F) -> Self {
        TryWith { inner, f }
    }
}

impl<S, F, R1, R2> Service<R1> for TryWith<S, F>
where
    S: Service<R2>,
    F: FnOnce(R1) -> Result<R2, S::Error> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Either<S::Future, Ready<Result<S::Response, S::Error>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: R1) -> Self::Future {
        match (self.f.clone())(request) {
            Ok(ok) => Either::Left(self.inner.call(ok)),
            Err(err) => Either::Right(ready(Err(err))),
        }
    }
}

/// A [`Layer`] that produces a [`TryWith`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Debug)]
pub struct TryWithLayer<F> {
    f: F,
}

impl<F> TryWithLayer<F> {
    /// Creates a new [`TryWithLayer`].
    pub fn new(f: F) -> Self {
        TryWithLayer { f }
    }
}

impl<S, F> Layer<S> for TryWithLayer<F>
where
    F: Clone,
{
    type Service = TryWith<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        TryWith {
            f: self.f.clone(),
            inner,
        }
    }
}
