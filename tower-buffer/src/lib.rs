//! Buffer requests when the inner service is out of capacity.
//!
//! Buffering works by spawning a new task that is dedicated to pulling requests
//! out of the buffer and dispatching them to the inner service. By adding a
//! buffer and a dedicated task, the `Buffer` layer in front of the service can
//! be `Clone` even if the inner service is not.

#[macro_use]
extern crate futures;
extern crate tokio_executor;
extern crate tokio_sync;
extern crate tower_service;

pub mod error;
pub mod future;
mod message;
mod worker;

pub use worker::WorkerExecutor;

use error::{Error, SpawnError};
use message::Message;
use future::ResponseFuture;
use worker::Worker;

use futures::Poll;
use tokio_executor::DefaultExecutor;
use tokio_sync::mpsc;
use tokio_sync::oneshot;
use tower_service::Service;

/// Adds a buffer in front of an inner service.
///
/// See crate level documentation for more details.
pub struct Buffer<T, Request>
where
    T: Service<Request>,
{
    tx: mpsc::Sender<Message<Request, T::Future, T::Error>>,
    worker: worker::Handle<T::Error>,
}

impl<T, Request> Buffer<T, Request>
where
    T: Service<Request>,
{
    /// Creates a new `Buffer` wrapping `service`.
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    ///
    /// The default Tokio executor is used to run the given service, which means that this method
    /// must be called while on the Tokio runtime.
    pub fn new(service: T, bound: usize) -> Result<Self, SpawnError<T>>
    where
        T: Send + 'static,
        T::Future: Send,
        T::Error: Send + Sync,
        Request: Send + 'static,
    {
        Self::with_executor(service, bound, &DefaultExecutor::current())
    }

    /// Creates a new `Buffer` wrapping `service`.
    ///
    /// `executor` is used to spawn a new `Worker` task that is dedicated to
    /// draining the buffer and dispatching the requests to the internal
    /// service.
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    pub fn with_executor<E>(service: T, bound: usize, executor: &E) -> Result<Self, SpawnError<T>>
    where
        E: WorkerExecutor<T, Request>,
    {
        let (tx, rx) = mpsc::channel(bound);

        match Worker::spawn(service, rx, executor) {
            Ok(worker) => Ok(Buffer { tx, worker }),
            Err(service) => Err(SpawnError::new(service)),
        }
    }
}

impl<T, Request> Service<Request> for Buffer<T, Request>
where
    T: Service<Request>,
{
    type Response = T::Response;
    type Error = Error<T::Error>;
    type Future = ResponseFuture<T::Future, T::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // If the inner service has errored, then we error here.
        self.tx
            .poll_ready()
            .map_err(move |_| Error::Closed(self.worker.get_error_on_closed()))
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
                    ResponseFuture::failed(self.worker.get_error_on_closed())
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
