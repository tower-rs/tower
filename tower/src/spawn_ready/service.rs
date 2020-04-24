use super::future::background_ready;
use futures_core::ready;
use futures_util::future::{MapErr, TryFutureExt};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use tower_service::Service;

/// Spawns tasks to drive an inner service to readiness.
///
/// See crate level documentation for more details.
#[derive(Debug)]
pub struct SpawnReady<T> {
    inner: Inner<T>,
}

#[derive(Debug)]
enum Inner<T> {
    Service(Option<T>),
    Future(oneshot::Receiver<Result<T, crate::BoxError>>),
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
    T: Service<Request> + Send + 'static,
    T::Error: Into<crate::BoxError>,
    Request: Send + 'static,
{
    type Response = T::Response;
    type Error = crate::BoxError;
    type Future = MapErr<T::Future, fn(T::Error) -> crate::BoxError>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.inner = match self.inner {
                Inner::Service(ref mut svc) => {
                    if let Poll::Ready(r) = svc.as_mut().expect("illegal state").poll_ready(cx) {
                        return Poll::Ready(r.map_err(Into::into));
                    }

                    let (bg, rx) = background_ready(svc.take().expect("illegal state"));

                    tokio::spawn(bg);

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
