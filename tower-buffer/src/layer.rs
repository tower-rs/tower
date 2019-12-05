use crate::{error::Error, service::Buffer};
use std::{fmt, marker::PhantomData};
use tower_layer::Layer;
use tower_service::Service;

/// Buffer requests with a bounded buffer
pub struct BufferLayer<Request> {
    bound: usize,
    _p: PhantomData<fn(Request)>,
}

impl<Request> BufferLayer<Request> {
    /// Create a new `BufferLayer` with the provided `bound`.
    pub fn new(bound: usize) -> Self {
        BufferLayer {
            bound,
            _p: PhantomData,
        }
    }
}

impl<S, Request> Layer<S> for BufferLayer<Request>
where
    S: Service<Request> + Send + 'static,
    S::Future: Send,
    S::Error: Into<Error> + Send + Sync,
    Request: Send + 'static,
{
    type Service = Buffer<S, Request>;

    fn layer(&self, service: S) -> Self::Service {
        Buffer::new(service, self.bound)
    }
}

impl<Request> fmt::Debug for BufferLayer<Request> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("BufferLayer")
            .field("bound", &self.bound)
            .finish()
    }
}
