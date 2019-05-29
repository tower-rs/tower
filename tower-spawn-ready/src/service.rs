use crate::{
    error::{Error, SpawnError},
    future::{background_ready, BackgroundReadyExecutor},
};
use futures::{future, try_ready, Async, Future, Poll};
use std::marker::PhantomData;
use tokio_executor::DefaultExecutor;
use tokio_sync::oneshot;
use tower_service::Service;

/// Spawns tasks to drive an inner service to readiness.
///
/// See crate level documentation for more details.
pub struct SpawnReady<T, Request, E>
where
    T: Service<Request>,
    T::Error: Into<Error>,
    E: BackgroundReadyExecutor<T, Request>,
{
    executor: E,
    inner: Inner<T>,
    _marker: PhantomData<fn(Request)>,
}

enum Inner<T> {
    Service(Option<T>),
    Future(oneshot::Receiver<Result<T, Error>>),
}

impl<T, Request> SpawnReady<T, Request, DefaultExecutor>
where
    T: Service<Request> + Send + 'static,
    T::Error: Into<Error>,
    Request: 'static,
{
    /// Creates a new `SpawnReady` wrapping `service`.
    ///
    /// The default Tokio executor is used to drive service readiness, which
    /// means that this method must be called while on the Tokio runtime.
    pub fn new(service: T) -> Self {
        Self::with_executor(service, DefaultExecutor::current())
    }
}

impl<T, Request, E> SpawnReady<T, Request, E>
where
    T: Service<Request> + Send,
    T::Error: Into<Error>,
    E: BackgroundReadyExecutor<T, Request>,
{
    /// Creates a new `SpawnReady` wrapping `service`.
    ///
    /// `executor` is used to spawn a new `BackgroundReady` task that is
    /// dedicated to driving the inner service to readiness.
    pub fn with_executor(service: T, executor: E) -> Self {
        Self {
            executor,
            inner: Inner::Service(Some(service)),
            _marker: PhantomData,
        }
    }
}

impl<T, Request, E> Service<Request> for SpawnReady<T, Request, E>
where
    T: Service<Request> + Send,
    T::Error: Into<Error>,
    E: BackgroundReadyExecutor<T, Request>,
{
    type Response = T::Response;
    type Error = Error;
    type Future = future::MapErr<T::Future, fn(T::Error) -> Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        loop {
            self.inner = match self.inner {
                Inner::Service(ref mut svc) => {
                    if svc
                        .as_mut()
                        .expect("illegal state")
                        .poll_ready()
                        .map_err(Into::into)?
                        .is_ready()
                    {
                        return Ok(Async::Ready(()));
                    }

                    let (bg, rx) = background_ready(svc.take().expect("illegal state"));
                    self.executor.spawn(bg).map_err(SpawnError::new)?;

                    Inner::Future(rx)
                }
                Inner::Future(ref mut fut) => {
                    let svc = try_ready!(fut.poll())?;
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
