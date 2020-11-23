use std::marker::PhantomData;
use std::task::{Context, Poll};

use futures_core::TryFuture;
use futures_util::{future::AndThen as AndThenFut, TryFutureExt};
use tower_layer::Layer;
use tower_service::Service;

/// Service returned by the [`and_then`] combinator.
///
/// [`and_then`]: crate::util::ServiceExt::and_then
#[derive(Clone, Debug)]
pub struct AndThen<S, F, Fut> {
    inner: S,
    f: F,
    _fut: PhantomData<Fut>,
}

impl<S, F, Fut> AndThen<S, F, Fut> {
    /// Creates a new `AndThen` service.
    pub fn new(inner: S, f: F) -> Self {
        AndThen {
            f,
            inner,
            _fut: Default::default(),
        }
    }
}

impl<S, F, Request, Fut> Service<Request> for AndThen<S, F, Fut>
where
    S: Service<Request>,
    F: FnOnce(S::Response) -> Fut + Clone,
    Fut: TryFuture<Error = S::Error>,
{
    type Response = Fut::Ok;
    type Error = Fut::Error;
    type Future = AndThenFut<S::Future, Fut, F>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).and_then(self.f.clone())
    }
}

/// A [`Layer`] that produces a [`AndThen`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Debug)]
pub struct AndThenLayer<F, Fut> {
    f: F,
    _fut: PhantomData<Fut>,
}

impl<F, Fut> AndThenLayer<F, Fut> {
    /// Creates a new [`AndThenLayer`] layer.
    pub fn new(f: F) -> Self {
        AndThenLayer {
            f,
            _fut: Default::default(),
        }
    }
}

impl<S, F, Fut> Layer<S> for AndThenLayer<F, Fut>
where
    F: Clone,
{
    type Service = AndThen<S, F, Fut>;

    fn layer(&self, inner: S) -> Self::Service {
        AndThen {
            f: self.f.clone(),
            inner,
            _fut: Default::default(),
        }
    }
}
