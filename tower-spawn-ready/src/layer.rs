use crate::{error::Error, future::BackgroundReadyExecutor, MakeSpawnReady};
use std::{fmt, marker::PhantomData};
use tokio_executor::DefaultExecutor;
use tower_layer::Layer;
use tower_service::Service;

/// Spawns tasks to drive its inner service to readiness.
pub struct SpawnReadyLayer<Target, Request, E = DefaultExecutor> {
    executor: E,
    _p: PhantomData<fn(Target, Request)>,
}

impl<Target, Request> SpawnReadyLayer<Target, Request, DefaultExecutor> {
    pub fn new() -> Self {
        Self {
            executor: DefaultExecutor::current(),
            _p: PhantomData,
        }
    }
}

impl<Target, Request, E: Clone> SpawnReadyLayer<Target, Request, E> {
    pub fn with_executor(executor: E) -> Self {
        Self {
            executor,
            _p: PhantomData,
        }
    }
}

impl<E, S, Target, T, Request> Layer<S> for SpawnReadyLayer<Target, Request, E>
where
    S: Service<Target, Response = T>,
    T: Service<Request> + Send + 'static,
    T::Error: Into<Error>,
    E: BackgroundReadyExecutor<T, Request> + Clone,
{
    type Service = MakeSpawnReady<S, Target, Request, E>;

    fn layer(&self, service: S) -> Self::Service {
        MakeSpawnReady::with_executor(service, self.executor.clone())
    }
}

impl<Target, Request, E: Clone> Clone for SpawnReadyLayer<Target, Request, E> {
    fn clone(&self) -> Self {
        Self {
            executor: self.executor.clone(),
            _p: PhantomData,
        }
    }
}

impl<Target, Request, E> fmt::Debug for SpawnReadyLayer<Target, Request, E>
where
    // Require E: Debug in case we want to print the executor at a later date
    E: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SpawnReadyLayer").finish()
    }
}
