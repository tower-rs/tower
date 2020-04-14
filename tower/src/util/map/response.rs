use pin_project::pin_project;
use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tower_layer::Layer;
use tower_service::Service;

/// A [`tower::Service`] which maps from one response type to another.
///
/// [`tower::Service`]: ../trait.Service.html
#[derive(Debug)]
pub struct MapResponse<S, F> {
    inner: S,
    f: Arc<F>,
}

impl<S, F> MapResponse<S, F> {
    /// Create a new `MapResponse`.
    pub fn new(inner: S, f: F) -> Self {
        let f = Arc::new(f);
        MapResponse { inner, f }
    }
}

impl<S, F, Request, R1, R2> Service<Request> for MapResponse<S, F>
where
    S: Service<Request, Response = R1>,
    F: Fn(R1) -> R2,
{
    type Response = R2;
    type Error = S::Error;
    type Future = MapResponseFuture<F, S::Future, R1, R2, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let f = self.f.clone();
        let fut = self.inner.call(req);

        MapResponseFuture {
            f,
            fut,
            _pd: PhantomData,
        }
    }
}

impl<S: Clone, F> Clone for MapResponse<S, F> {
    fn clone(&self) -> Self {
        MapResponse {
            inner: self.inner.clone(),
            f: self.f.clone(),
        }
    }
}

/// A [`tower::Layer`] that accepts a [`Clone`]able [`Fn`] and applies it
/// to each inner service.
///
/// [`tower::Layer`]: ../trait.Layer.html
/// [`Clone`]: https://doc.rust-lang.org/std/clone/trait.Clone.html
/// [`Fn`]: https://doc.rust-lang.org/std/ops/trait.Fn.html
#[derive(Debug, Clone)]
pub struct MapResponseLayer<F> {
    f: F,
}

impl<F> MapResponseLayer<F> {
    /// Create a new `MapResponseLayer`.
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
        MapResponse::new(inner, self.f.clone())
    }
}

#[pin_project]
#[derive(Debug)]
pub struct MapResponseFuture<F, Fut, R1, R2, E> {
    f: Arc<F>,
    #[pin]
    fut: Fut,
    _pd: PhantomData<fn(R1, R2, E)>,
}

impl<F, Fut, R1, R2, E> Future for MapResponseFuture<F, Fut, R1, R2, E>
where
    F: Fn(R1) -> R2,
    Fut: Future<Output = Result<R1, E>>,
{
    type Output = Result<R2, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();

        let res = match futures_core::ready!(me.fut.poll(cx)) {
            Ok(r) => Ok((&me.f)(r)),
            Err(e) => Err(e),
        };

        Poll::Ready(res)
    }
}
