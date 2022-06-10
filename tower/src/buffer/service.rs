use super::{
    future::ResponseFuture,
    message::Message,
    worker::{Handle, Worker},
};

use futures_util::ready;
use std::future::Future;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::ReusableBoxFuture;
use tower_service::{Call, Service};

/// Adds an mpsc buffer in front of an inner service.
///
/// See the module documentation for more details.
#[derive(Debug)]
pub struct Buffer<Request, F> {
    tx: mpsc::Sender<Message<Request, F>>,
    needs_reserve: bool,
    reserve: ReusableBoxFuture<
        'static,
        Result<mpsc::OwnedPermit<Message<Request, F>>, mpsc::error::SendError<()>>,
    >,
    handle: Handle,
}

pub struct CallBuffer<Request, F> {
    permit: mpsc::OwnedPermit<Message<Request, F>>,
    handle: Handle,
}

impl<Req, F> Buffer<Req, F>
where
    F: 'static,
{
    /// Creates a new [`Buffer`] wrapping `service`.
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    ///
    /// The default Tokio executor is used to run the given service, which means that this method
    /// must be called while on the Tokio runtime.
    ///
    /// # A note on choosing a `bound`
    ///
    /// When [`Buffer`]'s implementation of [`poll_ready`] returns [`Poll::Ready`], it reserves a
    /// slot in the channel for the forthcoming [`call`]. However, if this call doesn't arrive,
    /// this reserved slot may be held up for a long time. As a result, it's advisable to set
    /// `bound` to be at least the maximum number of concurrent requests the [`Buffer`] will see.
    /// If you do not, all the slots in the buffer may be held up by futures that have just called
    /// [`poll_ready`] but will not issue a [`call`], which prevents other senders from issuing new
    /// requests.
    ///
    /// [`Poll::Ready`]: std::task::Poll::Ready
    /// [`call`]: crate::Service::call
    /// [`poll_ready`]: crate::Service::poll_ready
    pub fn new<S, E>(service: S, bound: usize) -> Self
    where
        S: for<'a> Service<'a, Req, Future = F, Error = E>,
        S: Send + 'static,
        F: Send,
        Req: Send + 'static,
        F: Send,
        E: Into<crate::BoxError> + Send + Sync,
    {
        let (service, worker) = Self::pair(service, bound);
        tokio::spawn(worker);
        service
    }

    /// Creates a new [`Buffer`] wrapping `service`, but returns the background worker.
    ///
    /// This is useful if you do not want to spawn directly onto the tokio runtime
    /// but instead want to use your own executor. This will return the [`Buffer`] and
    /// the background `Worker` that you can then spawn.
    pub fn pair<S, E>(service: S, bound: usize) -> (Self, Worker<S, Req, F>)
    where
        S: for<'a> Service<'a, Req, Future = F, Error = E>,
        S: Send + 'static,
        F: Send,
        Req: Send + 'static,
        F: Send,
        E: Into<crate::BoxError> + Send + Sync,
    {
        let (tx, rx) = mpsc::channel(bound);
        let (handle, worker) = Worker::new(service, rx);
        let buffer = Buffer {
            tx,
            handle,
            needs_reserve: true,
            reserve: ReusableBoxFuture::new(async move {
                unreachable!("initial reserve future is never called")
            }),
        };
        (buffer, worker)
    }

    fn get_worker_error(&self) -> crate::BoxError {
        self.handle.get_error_on_closed()
    }
}

impl<'a, Req, Rsp, E, F> Service<'a, Req> for Buffer<Req, F>
where
    F: Future<Output = Result<Rsp, E>> + Send + 'static,
    E: Into<crate::BoxError>,
    Req: Send + 'static,
{
    type Call = CallBuffer<Req, F>;
    type Response = Rsp;
    type Error = crate::BoxError;
    type Future = ResponseFuture<F>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Call, Self::Error>> {
        // First, check if the worker is still alive.
        if self.tx.is_closed() {
            // If the inner service has errored, then we error here.
            return Poll::Ready(Err(self.get_worker_error()));
        }

        // Then, check if we've already reserved a permit.
        if self.needs_reserve {
            self.reserve.set(self.tx.clone().reserve_owned());
            self.needs_reserve = false;
        }

        // Finally, if we haven't already reserved a permit, poll the semaphore
        // to reserve one. If we reserve a permit, then there's enough buffer
        // capacity to send a new request. Otherwise, we need to wait for
        // capacity.
        let res = ready!(self.reserve.poll(cx));
        self.needs_reserve = true;

        match res {
            Ok(permit) => Poll::Ready(Ok(CallBuffer {
                permit,
                handle: self.handle.clone(),
            })),
            Err(_) => Poll::Ready(Err(self.handle.get_error_on_closed())),
        }
    }

    // fn call(&mut self, request: Req) -> Self::Future {
    //     tracing::trace!("sending request to buffer worker");

    //     // get the current Span so that we can explicitly propagate it to the worker
    //     // if we didn't do this, events on the worker related to this span wouldn't be counted
    //     // towards that span since the worker would have no way of entering it.
    //     let span = tracing::Span::current();

    //     // If we've made it here, then a channel permit has already been
    //     // acquired, so we can freely allocate a oneshot.
    //     let (tx, rx) = oneshot::channel();

    //     match self.tx.send_item(Message { request, span, tx }) {
    //         Ok(_) => ResponseFuture::new(rx),
    //         // If the channel is closed, propagate the error from the worker.
    //         Err(_) => {
    //             tracing::trace!("buffer channel closed");
    //             ResponseFuture::failed(self.get_worker_error())
    //         }
    //     }
    // }
}

impl<Req, Rsp, Err, F> Call<Req> for CallBuffer<Req, F>
where
    Err: Into<crate::BoxError>,
    F: Future<Output = Result<Rsp, Err>>,
{
    type Response = Rsp;
    type Error = crate::BoxError;
    type Future = ResponseFuture<F>;

    fn call(self, request: Req) -> Self::Future {
        tracing::trace!("sending request to buffer worker");
        if let Some(err) = self.handle.try_get_error() {
            return ResponseFuture::failed(err);
        }

        // get the current Span so that we can explicitly propagate it to the worker
        // if we didn't do this, events on the worker related to this span wouldn't be counted
        // towards that span since the worker would have no way of entering it.
        let span = tracing::Span::current();

        // If we've made it here, then a semaphore permit has already been
        // reserved, so we can freely allocate a oneshot.
        let (tx, rx) = oneshot::channel();

        self.permit.send(Message { request, span, tx });
        ResponseFuture::new(rx)
    }
}

impl<Request, F> Clone for Buffer<Request, F> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            handle: self.handle.clone(),
            reserve: ReusableBoxFuture::new(async { unreachable!() }),
            needs_reserve: true,
        }
    }
}
