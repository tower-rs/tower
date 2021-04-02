use super::spawn::{Spawn, SpawnFuture};
use crate::{
    buffer::{future::ResponseFuture, Buffer},
    BoxError,
};
use pin_project::pin_project;
use std::{
    fmt::{self, Debug, Formatter},
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::runtime::Handle;
use tower_service::Service;

/// Execute a service on a remote tokio executor.
///
/// See the module documentation for more details.
#[derive(Clone)]
pub struct Remote<T, R>
where
    T: Service<R>,
    T::Future: Send + 'static,
    T::Response: Send + 'static,
    T::Error: 'static,
    BoxError: From<T::Error>,
{
    inner: Buffer<Spawn<T>, R>,
}

/// A future that resolves to the response produced on the remote executor.
#[pin_project]
#[derive(Debug)]
pub struct RemoteFuture<T> {
    // Newtype around Buffer's future to hide the fact that we're using it under the hood.
    #[pin]
    inner: ResponseFuture<SpawnFuture<T>>,
}

impl<T, R> Remote<T, R>
where
    T: Service<R> + Send + 'static,
    T::Future: Send + 'static,
    T::Response: Send + 'static,
    T::Error: 'static,
    R: Send + 'static,
    BoxError: From<T::Error>,
{
    /// Creates a new [`Remote`] wrapping `service` that spawns onto the current tokio runtime.
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    ///
    /// The current Tokio executor is used to run the given service, which means that this method
    /// must be called while on the Tokio runtime.
    pub fn new(service: T, bound: usize) -> Self {
        Self::with_handle(service, bound, &Handle::current())
    }

    /// Creates a new [`Remote`] wrapping `service`, spawning onto the runtime that is connected
    /// to the given [`Handle`].
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    pub fn with_handle(service: T, bound: usize, handle: &Handle) -> Self {
        let (inner, worker) = Buffer::pair(Spawn::new(service, handle.clone()), bound);
        handle.spawn(worker);

        Self { inner }
    }
}

impl<T, R> Debug for Remote<T, R>
where
    T: Service<R>,
    T::Future: Send + 'static,
    T::Response: Send + 'static,
    T::Error: 'static,
    BoxError: From<T::Error>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Remote").finish()
    }
}

impl<T, R> Service<R> for Remote<T, R>
where
    T: Service<R>,
    T::Future: Send + 'static,
    T::Response: Send + 'static,
    T::Error: 'static,
    BoxError: From<T::Error>,
{
    type Response = T::Response;
    type Error = BoxError;
    type Future = RemoteFuture<T::Response>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: R) -> Self::Future {
        RemoteFuture {
            inner: self.inner.call(req),
        }
    }
}

impl<T> Future for RemoteFuture<T> {
    type Output = Result<T, BoxError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        this.inner.poll(cx)
    }
}
