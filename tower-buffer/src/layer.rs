use crate::{error::Error, service::Buffer, worker::WorkerExecutor};
use tokio_executor::DefaultExecutor;
use tower_layer::Layer;
use tower_service::Service;

/// Buffer requests with a bounded buffer
pub struct BufferLayer<E = DefaultExecutor> {
    bound: usize,
    executor: E,
}

impl BufferLayer<DefaultExecutor> {
    pub fn new(bound: usize) -> Self {
        BufferLayer {
            bound,
            executor: DefaultExecutor::current(),
        }
    }
}

impl<E> BufferLayer<E> {
    pub fn with_executor<S, Request>(bound: usize, executor: E) -> Self
    where
        S: Service<Request>,
        S::Error: Into<Error>,
        E: WorkerExecutor<S, Request> + Clone,
    {
        BufferLayer { bound, executor }
    }
}

impl<E, S, Request> Layer<S, Request> for BufferLayer<E>
where
    S: Service<Request>,
    S::Error: Into<Error>,
    E: WorkerExecutor<S, Request> + Clone,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = Error;
    type Service = Buffer<S, Request>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(Buffer::with_executor(
            service,
            self.bound,
            &mut self.executor.clone(),
        ))
    }
}
