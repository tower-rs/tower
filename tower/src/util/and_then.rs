use std::task::{Context, Poll};

use futures_core::TryFuture;
use futures_util::{
    future::{AndThen as AndThenFut, ErrInto as ErrIntoFut},
    TryFutureExt,
};
use tower_layer::Layer;
use tower_service::Service;

/// Service returned by the [`and_then`] combinator.
///
/// [`and_then`]: crate::util::ServiceExt::and_then
#[derive(Clone, Debug)]
pub struct AndThen<S, F> {
    inner: S,
    f: F,
}

/// A [`Layer`] that produces a [`AndThen`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Debug)]
pub struct AndThenLayer<F> {
    f: F,
}

impl<S, F> AndThen<S, F> {
    /// Creates a new `AndThen` service.
    pub fn new(inner: S, f: F) -> Self {
        AndThen { f, inner }
    }
}

impl<S, F, Request, Fut> Service<Request> for AndThen<S, F>
where
    S: Service<Request>,
    S::Error: Into<Fut::Error>,
    F: FnOnce(S::Response) -> Fut + Clone,
    Fut: TryFuture,
{
    type Response = Fut::Ok;
    type Error = Fut::Error;
    type Future = AndThenFut<ErrIntoFut<S::Future, Fut::Error>, Fut, F>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).err_into().and_then(self.f.clone())
    }
}

impl<F> AndThenLayer<F> {
    /// Creates a new [`AndThenLayer`] layer.
    pub fn new(f: F) -> Self {
        AndThenLayer { f }
    }
}

impl<S, F> Layer<S> for AndThenLayer<F>
where
    F: Clone,
{
    type Service = AndThen<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        AndThen {
            f: self.f.clone(),
            inner,
        }
    }
}
