use crate::SpawnReady;
use futures::{try_ready, Future, Poll};
use tokio_executor::DefaultExecutor;
use tower_service::Service;

pub struct MakeSpawnReady<S, E = DefaultExecutor> {
    inner: S,
    executor: E,
}

pub struct MakeFuture<F, E = DefaultExecutor> {
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

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, target: Target) -> Self::Future {
        MakeFuture {
            inner: self.inner.call(target),
            executor: self.executor.clone(),
        }
    }
}

impl<F, E> Future for MakeFuture<F, E>
where
    F: Future,
    E: Clone,
{
    type Item = SpawnReady<F::Item, E>;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let inner = try_ready!(self.inner.poll());
        let svc = SpawnReady::with_executor(inner, self.executor.clone());
        Ok(svc.into())
    }
}
