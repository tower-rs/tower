use futures_util::FutureExt;
use std::task::{Context, Poll};
use tower_layer::Layer;
use tower_service::Service;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use futures_util::future::Map as ThenFuture;

/// Service returned by the [`then`] combinator.
///
/// [`then`]: crate::util::ServiceExt::then
#[derive(Clone, Debug)]
pub struct Then<S, F> {
    inner: S,
    f: F,
}

/// A [`Layer`] that produces a [`Then`] service.
///
/// [`Layer`]: tower_layer::Layer
#[derive(Debug, Clone)]
pub struct ThenLayer<F> {
    f: F,
}

impl<S, F> Then<S, F> {
    /// Creates a new `Then` service.
    pub fn new(inner: S, f: F) -> Self {
        Then { f, inner }
    }
}

impl<S, F, Request, Response, Error> Service<Request> for Then<S, F>
where
    S: Service<Request>,
    Error: From<S::Error>,
    F: FnOnce(Result<S::Response, S::Error>) -> Result<Response, Error> + Clone,
{
    type Response = Response;
    type Error = Error;
    type Future = ThenFuture<S::Future, F>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    #[inline]
    fn call(&mut self, request: Request) -> Self::Future {
        self.inner.call(request).map(self.f.clone())
    }
}

impl<F> ThenLayer<F> {
    /// Creates a new [`ThenLayer`] layer.
    pub fn new(f: F) -> Self {
        ThenLayer { f }
    }
}

impl<S, F> Layer<S> for ThenLayer<F>
where
    F: Clone,
{
    type Service = Then<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        Then {
            f: self.f.clone(),
            inner,
        }
    }
}
