use crate::{
    error::{Error, SpawnError},
    future::ResponseFuture,
    message::Message,
    worker::{Handle, Worker, WorkerExecutor},
};

use futures::Poll;
use tokio_executor::DefaultExecutor;
use tokio_sync::{mpsc, oneshot};
use tower_service::Service;

/// Adds a buffer in front of an inner service.
///
/// See crate level documentation for more details.
pub struct Buffer<T, Request>
where
    T: Service<Request>,
{
    tx: mpsc::Sender<Message<Request, T::Future>>,
    worker: Option<Handle>,
}

impl<T, Request> Buffer<T, Request>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    /// Creates a new `Buffer` wrapping `service`.
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    ///
    /// The default Tokio executor is used to run the given service, which means that this method
    /// must be called while on the Tokio runtime.
    pub fn new(service: T, bound: usize) -> Self
    where
        T: Send + 'static,
        T::Future: Send,
        T::Error: Send + Sync,
        Request: Send + 'static,
    {
        Self::with_executor(service, bound, &mut DefaultExecutor::current())
    }

    /// Creates a new `Buffer` wrapping `service`.
    ///
    /// `executor` is used to spawn a new `Worker` task that is dedicated to
    /// draining the buffer and dispatching the requests to the internal
    /// service.
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    pub fn with_executor<E>(service: T, bound: usize, executor: &mut E) -> Self
    where
        E: WorkerExecutor<T, Request>,
    {
        let (tx, rx) = mpsc::channel(bound);
        let worker = Worker::spawn(service, rx, executor);
        Buffer { tx, worker }
    }

    fn get_worker_error(&self) -> Error {
        self.worker
            .as_ref()
            .map(|w| w.get_error_on_closed())
            .unwrap_or_else(|| {
                // If there's no worker handle, that's because spawning it
                // at the beginning failed.
                SpawnError::new().into()
            })
    }
}

impl<T, Request> Service<Request> for Buffer<T, Request>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    type Response = T::Response;
    type Error = Error;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // If the inner service has errored, then we error here.
        self.tx.poll_ready().map_err(|_| self.get_worker_error())
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // TODO:
        // ideally we'd poll_ready again here so we don't allocate the oneshot
        // if the try_send is about to fail, but sadly we can't call poll_ready
        // outside of task context.
        let (tx, rx) = oneshot::channel();

        match self.tx.try_send(Message { request, tx }) {
            Err(e) => {
                if e.is_closed() {
                    ResponseFuture::failed(self.get_worker_error())
                } else {
                    // When `mpsc::Sender::poll_ready` returns `Ready`, a slot
                    // in the channel is reserved for the handle. Other `Sender`
                    // handles may not send a message using that slot. This
                    // guarantees capacity for `request`.
                    //
                    // Given this, the only way to hit this code path is if
                    // `poll_ready` has not been called & `Ready` returned.
                    panic!("buffer full; poll_ready must be called first");
                }
            }
            Ok(_) => ResponseFuture::new(rx),
        }
    }
}

impl<T, Request> Clone for Buffer<T, Request>
where
    T: Service<Request>,
{
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            worker: self.worker.clone(),
        }
    }
}
