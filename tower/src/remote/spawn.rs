use crate::BoxError;
use futures_core::ready;
use futures_util::TryFutureExt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{runtime::Handle, task::JoinHandle};
use tower_service::Service;

/// A service that spawns the future from the inner service on the current tokio
/// executor.
#[derive(Clone, Debug)]
pub(crate) struct Spawn<T> {
    handle: Handle,
    inner: T,
}

/// A future that covers the execution of the spawned service future.
#[derive(Debug)]
pub(crate) struct SpawnFuture<T> {
    inner: JoinHandle<Result<T, BoxError>>,
}

impl<T> Spawn<T> {
    /// Creates a new spawn service.
    pub(crate) fn new(service: T, handle: Handle) -> Self {
        Self {
            inner: service,
            handle,
        }
    }
}

impl<T, R> Service<R> for Spawn<T>
where
    T: Service<R>,
    T::Future: Send + 'static,
    T::Response: Send + 'static,
    T::Error: 'static,
    BoxError: From<T::Error>,
{
    type Response = T::Response;
    type Error = BoxError;
    type Future = SpawnFuture<T::Response>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: R) -> Self::Future {
        let future = self.inner.call(req).map_err(BoxError::from);
        let spawned = self.handle.spawn(future);
        SpawnFuture { inner: spawned }
    }
}

impl<T> Future for SpawnFuture<T> {
    type Output = Result<T, BoxError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let response = ready!(Pin::new(&mut self.inner).poll(cx))??;
        Poll::Ready(Ok(response))
    }
}
