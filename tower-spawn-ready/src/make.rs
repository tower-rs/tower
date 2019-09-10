use crate::SpawnReady;
use futures_core::ready;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_executor::DefaultExecutor;
use tower_service::Service;

/// Builds SpawnReady instances with the result of an inner Service.
#[derive(Clone, Debug)]
pub struct MakeSpawnReady<S, E = DefaultExecutor> {
    inner: S,
    executor: E,
}

/// Builds a SpawnReady with the result of an inner Future.
#[pin_project]
#[derive(Debug)]
pub struct MakeFuture<F, E = DefaultExecutor> {
    #[pin]
    inner: F,
    executor: E,
}

impl<S, E> MakeSpawnReady<S, E> {
    pub(crate) fn with_executor(inner: S, executor: E) -> Self {
        Self { inner, executor }
    }
}

impl<S, E, Target> Service<Target> for MakeSpawnReady<S, E>
where
    S: Service<Target>,
    E: Clone,
{
    type Response = SpawnReady<S::Response, E>;
    type Error = S::Error;
    type Future = MakeFuture<S::Future, E>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, target: Target) -> Self::Future {
        MakeFuture {
            inner: self.inner.call(target),
            executor: self.executor.clone(),
        }
    }
}

impl<F, T, E, X> Future for MakeFuture<F, X>
where
    F: Future<Output = Result<T, E>>,
    X: Clone,
{
    type Output = Result<SpawnReady<T, X>, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let inner = ready!(this.inner.poll(cx))?;
        let svc = SpawnReady::with_executor(inner, this.executor.clone());
        Poll::Ready(Ok(svc))
    }
}
