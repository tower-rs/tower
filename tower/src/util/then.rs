use futures_util::{future, FutureExt};
use std::{
    fmt,
    future::Future,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::{Call, Service};

/// [`Service`] returned by the [`then`] combinator.
///
/// [`then`]: crate::util::ServiceExt::then
#[derive(Clone)]
pub struct Then<S, F> {
    inner: S,
    f: F,
}

impl<S, F> fmt::Debug for Then<S, F>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Then")
            .field("inner", &self.inner)
            .field("f", &format_args!("{}", std::any::type_name::<F>()))
            .finish()
    }
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

    /// Returns a new [`Layer`] that produces [`Then`] services.
    ///
    /// This is a convenience function that simply calls [`ThenLayer::new`].
    ///
    /// [`Layer`]: tower_layer::Layer
    pub fn layer(f: F) -> ThenLayer<F> {
        ThenLayer { f }
    }
}

opaque_future! {
    /// Response future from [`Then`] services.
    ///
    /// [`Then`]: crate::util::Then
    pub type ThenFuture<F1, F2, N> = future::Then<F1, F2, N>;
}

impl<S, F, Request, Response, Error, Fut> Call<Request> for Then<S, F>
where
    S: Call<Request>,
    S::Error: Into<Error>,
    F: FnOnce(Result<S::Response, S::Error>) -> Fut + Clone,
    Fut: Future<Output = Result<Response, Error>>,
{
    type Response = Response;
    type Error = Error;
    type Future = ThenFuture<S::Future, Fut, F>;

    #[inline]
    fn call(self, request: Request) -> Self::Future {
        ThenFuture(self.inner.call(request).then(self.f.clone()))
    }
}

impl<'a, S, F, Request, Response, Fut, Error> Service<'a, Request> for Then<S, F>
where
    S: Service<'a, Request>,
    S::Error: Into<Error>,
    F: FnOnce(Result<S::Response, S::Error>) -> Fut + Clone,
    Fut: Future<Output = Result<Response, Error>>,
{
    type Call = Then<S::Call, F>;
    type Response = Response;
    type Error = Error;
    type Future = ThenFuture<S::Future, Fut, F>;

    fn poll_ready(&'a mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Call, Self::Error>> {
        let f = self.f.clone();
        self.inner
            .poll_ready(cx)
            .map_err(Into::into)
            .map_ok(|inner| Then { inner, f })
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
