use super::{
    future::ResponseFuture,
    message::Message,
    worker::{Handle, Worker},
};

use futures_core::ready;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, oneshot};
use tower_service::Service;

/// Adds an mpsc buffer in front of an inner service.
///
/// See the module documentation for more details.
#[derive(Debug)]
pub struct Buffer<T, Request>
where
    T: Service<Request>,
{
    tx: mpsc::Sender<Message<Request, T::Future>>,
    handle: Handle,
}

impl<T, Request> Buffer<T, Request>
where
    T: Service<Request>,
    T::Error: Into<crate::BoxError>,
{
    /// Creates a new `Buffer` wrapping `service`.
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    ///
    /// The default Tokio executor is used to run the given service, which means that this method
    /// must be called while on the Tokio runtime.
    ///
    /// # A note on choosing a `bound`
    ///
    /// When `Buffer`'s implementation of `poll_ready` returns `Poll::Ready`, it reserves a
    /// slot in the channel for the forthcoming `call()`. However, if this call doesn't arrive,
    /// this reserved slot may be held up for a long time. As a result, it's advisable to set
    /// `bound` to be at least the maximum number of concurrent requests the `Buffer` will see.
    /// If you do not, all the slots in the buffer may be held up by futures that have just called
    /// `poll_ready` but will not issue a `call`, which prevents other senders from issuing new
    /// requests.
    pub fn new(service: T, bound: usize) -> Self
    where
        T: Send + 'static,
        T::Future: Send,
        T::Error: Send + Sync,
        Request: Send + 'static,
    {
        let (tx, rx) = mpsc::channel(bound);
        let (handle, worker) = Worker::new(service, rx);
        tokio::spawn(worker);
        Buffer { tx, handle }
    }

    /// Creates a new `Buffer` wrapping `service`, but returns the background worker.
    ///
    /// This is useful if you do not want to spawn directly onto the `tokio` runtime
    /// but instead want to use your own executor. This will return the `Buffer` and
    /// the background `Worker` that you can then spawn.
    pub fn pair(service: T, bound: usize) -> (Buffer<T, Request>, Worker<T, Request>)
    where
        T: Send + 'static,
        T::Error: Send + Sync,
        Request: Send + 'static,
    {
        let (tx, rx) = mpsc::channel(bound);
        let (handle, worker) = Worker::new(service, rx);
        (Buffer { tx, handle }, worker)
    }

    fn get_worker_error(&self) -> crate::BoxError {
        self.handle.get_error_on_closed()
    }
}

impl<T, Request> Service<Request> for Buffer<T, Request>
where
    T: Service<Request>,
    T::Error: Into<crate::BoxError>,
{
    type Response = T::Response;
    type Error = crate::BoxError;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // If the inner service has errored, then we error here.
        if let Err(_) = ready!(self.tx.poll_ready(cx)) {
            Poll::Ready(Err(self.get_worker_error()))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // TODO:
        // ideally we'd poll_ready again here so we don't allocate the oneshot
        // if the try_send is about to fail, but sadly we can't call poll_ready
        // outside of task context.
        let (tx, rx) = oneshot::channel();

        // get the current Span so that we can explicitly propagate it to the worker
        // if we didn't do this, events on the worker related to this span wouldn't be counted
        // towards that span since the worker would have no way of entering it.
        let span = tracing::Span::current();
        tracing::trace!(parent: &span, "sending request to buffer worker");
        match self.tx.try_send(Message { request, span, tx }) {
            Err(mpsc::error::TrySendError::Closed(_)) => {
                ResponseFuture::failed(self.get_worker_error())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // When `mpsc::Sender::poll_ready` returns `Ready`, a slot
                // in the channel is reserved for the handle. Other `Sender`
                // handles may not send a message using that slot. This
                // guarantees capacity for `request`.
                //
                // Given this, the only way to hit this code path is if
                // `poll_ready` has not been called & `Ready` returned.
                panic!("buffer full; poll_ready must be called first");
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
            handle: self.handle.clone(),
        }
    }
}
