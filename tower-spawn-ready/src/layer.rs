use crate::{error::Error, future::BackgroundReadyExecutor, service::SpawnReady};
use std::{fmt, marker::PhantomData};
use tokio_executor::DefaultExecutor;
use tower_layer::Layer;
use tower_service::Service;

/// Spawns tasks to drive its inner service to readiness.
pub struct SpawnReadyLayer<Request, E = DefaultExecutor> {
    executor: E,
    _p: PhantomData<fn(Request)>,
}

impl<Request> SpawnReadyLayer<Request, DefaultExecutor> {
    pub fn new() -> Self {
        SpawnReadyLayer {
            executor: DefaultExecutor::current(),
            _p: PhantomData,
        }
    }
}

impl<Request, E: Clone> SpawnReadyLayer<Request, E> {
    pub fn with_executor(executor: E) -> Self {
        SpawnReadyLayer {
            executor,
            _p: PhantomData,
        }
    }
}

impl<E, S, Request> Layer<S> for SpawnReadyLayer<Request, E>
where
    S: Service<Request> + Send + 'static,
    S::Error: Into<Error>,
    E: BackgroundReadyExecutor<S, Request> + Clone,
{
    type Service = SpawnReady<S, Request, E>;

    fn layer(&self, service: S) -> Self::Service {
        SpawnReady::with_executor(service, self.executor.clone())
    }
}

impl<Request, E> fmt::Debug for SpawnReadyLayer<Request, E>
where
    // Require E: Debug in case we want to print the executor at a later date
    E: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SpawnReadyLayer").finish()
    }
}
