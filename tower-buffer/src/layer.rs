use crate::{error::Error, service::Buffer, worker::WorkerExecutor};
use std::marker::PhantomData;
use tokio_executor::DefaultExecutor;
use tower_layer::Layer;
use tower_service::Service;

/// Buffer requests with a bounded buffer
pub struct BufferLayer<Request, E = DefaultExecutor> {
    bound: usize,
    executor: E,
    _p: PhantomData<fn(Request)>,
}

impl<Request> BufferLayer<Request, DefaultExecutor> {
    pub fn new(bound: usize) -> Self {
        BufferLayer {
            bound,
            executor: DefaultExecutor::current(),
            _p: PhantomData,
        }
    }
}

impl<Request, E: Clone> BufferLayer<Request, E> {
    pub fn with_executor(bound: usize, executor: E) -> Self {
        BufferLayer {
            bound,
            executor,
            _p: PhantomData,
        }
    }
}

impl<E, S, Request> Layer<S> for BufferLayer<Request, E>
where
    S: Service<Request>,
    S::Error: Into<Error>,
    E: WorkerExecutor<S, Request> + Clone,
{
    type Service = Buffer<S, Request>;

    fn layer(&self, service: S) -> Self::Service {
        Buffer::with_executor(service, self.bound, &mut self.executor.clone())
    }
}
