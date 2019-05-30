use crate::{error, future::BackgroundReadyExecutor, SpawnReady};
use futures::{try_ready, Future, Poll};
use std::marker::PhantomData;
use tokio_executor::DefaultExecutor;
use tower_service::Service;

pub struct MakeSpawnReady<S, Target, Request, E = DefaultExecutor> {
    inner: S,
    executor: E,
    _marker: PhantomData<fn(Target, Request)>,
}

pub struct MakeFuture<F, Request, E = DefaultExecutor> {
    inner: F,
    executor: E,
    _marker: PhantomData<fn(Request)>,
}

impl<S, Target, Request, E> MakeSpawnReady<S, Target, Request, E>
where
    S: Service<Target>,
    E: Clone,
{
    pub(crate) fn with_executor(inner: S, executor: E) -> Self {
        Self {
            inner,
            executor,
            _marker: PhantomData,
        }
    }
}

impl<S, Target, T, Request, E> Service<Target> for MakeSpawnReady<S, Target, Request, E>
where
    S: Service<Target, Response = T>,
    T: Service<Request> + Send,
    T::Error: Into<error::Error>,
    E: BackgroundReadyExecutor<T, Request> + Clone,
{
    type Response = SpawnReady<S::Response, Request, E>;
    type Error = S::Error;
    type Future = MakeFuture<S::Future, Request, E>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, target: Target) -> Self::Future {
        MakeFuture {
            inner: self.inner.call(target),
            executor: self.executor.clone(),
            _marker: PhantomData,
        }
    }
}

impl<S, F, Request, E> Future for MakeFuture<F, Request, E>
where
    F: Future<Item = S>,
    S: Service<Request> + Send,
    S::Error: Into<error::Error>,
    E: BackgroundReadyExecutor<S, Request> + Clone,
{
    type Item = SpawnReady<F::Item, Request, E>;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let inner = try_ready!(self.inner.poll());
        let svc = SpawnReady::with_executor(inner, self.executor.clone());
        Ok(svc.into())
    }
}
