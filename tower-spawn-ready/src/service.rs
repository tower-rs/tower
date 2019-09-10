use crate::{
    error::{Error, SpawnError},
    future::{background_ready, BackgroundReadyExecutor},
};
use futures_core::ready;
use futures_util::try_future::{MapErr, TryFutureExt};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_executor::{DefaultExecutor, TypedExecutor};
use tokio_sync::oneshot;
use tower_service::Service;

/// Spawns tasks to drive an inner service to readiness.
///
/// See crate level documentation for more details.
pub struct SpawnReady<T> {
    inner: Inner<T>,
}

enum Inner<T> {
    Service(Option<T>),
    Future(oneshot::Receiver<Result<T, Error>>),
}

impl<T> SpawnReady<T> {
    /// Creates a new `SpawnReady` wrapping `service`.
    pub fn new(service: T) -> Self {
        Self {
            inner: Inner::Service(Some(service)),
        }
    }
}

impl<T, Request> Service<Request> for SpawnReady<T>
where
    T: Service<Request> + Send,
    T::Error: Into<Error>,
    DefaultExecutor: BackgroundReadyExecutor<T, Request>,
{
    type Response = T::Response;
    type Error = Error;
    type Future = MapErr<T::Future, fn(T::Error) -> Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.inner = match self.inner {
                Inner::Service(ref mut svc) => {
                    if let Poll::Ready(r) = svc.as_mut().expect("illegal state").poll_ready(cx) {
                        return Poll::Ready(r.map_err(Into::into));
                    }

                    let (bg, rx) = background_ready(svc.take().expect("illegal state"));
                    DefaultExecutor::current()
                        .spawn(bg)
                        .map_err(SpawnError::new)?;

                    Inner::Future(rx)
                }
                Inner::Future(ref mut fut) => {
                    let svc = ready!(Pin::new(fut).poll(cx))??;
                    Inner::Service(Some(svc))
                }
            }
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        match self.inner {
            Inner::Service(Some(ref mut svc)) => svc.call(request).map_err(Into::into),
            _ => unreachable!("poll_ready must be called"),
        }
    }
}
