use crate::MakeSpawnReady;
use std::fmt;
use tokio_executor::DefaultExecutor;
use tower_layer::Layer;

/// Spawns tasks to drive its inner service to readiness.
pub struct SpawnReadyLayer<E = DefaultExecutor> {
    executor: E,
}

impl SpawnReadyLayer<DefaultExecutor> {
    /// Builds a SpawnReady layer with the default executor.
    pub fn new() -> Self {
        Self {
            executor: DefaultExecutor::current(),
        }
    }
}

impl<E: Clone> SpawnReadyLayer<E> {
    /// Builds a SpawnReady layer with the provided executor.
    pub fn with_executor(executor: E) -> Self {
        Self { executor }
    }
}

impl<E, S> Layer<S> for SpawnReadyLayer<E>
where
    E: Clone,
{
    type Service = MakeSpawnReady<S, E>;

    fn layer(&self, service: S) -> Self::Service {
        MakeSpawnReady::with_executor(service, self.executor.clone())
    }
}

impl<E: Clone> Clone for SpawnReadyLayer<E> {
    fn clone(&self) -> Self {
        Self {
            executor: self.executor.clone(),
        }
    }
}

impl<E> fmt::Debug for SpawnReadyLayer<E>
where
    // Require E: Debug in case we want to print the executor at a later date
    E: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SpawnReadyLayer").finish()
    }
}
