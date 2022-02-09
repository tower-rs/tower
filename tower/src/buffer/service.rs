use super::{
    future::ResponseFuture,
    message::Message,
    worker::{Handle, Worker},
};

use futures_core::ready;
use std::{
    fmt, mem,
    task::{Context, Poll},
};
use tokio::sync::{
    mpsc::{self, error::SendError},
    oneshot,
};
use tokio_util::sync::ReusableBoxFuture;
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
    /// A future attempting to acquire a permit to send a message to the mpsc
    /// channel.
    ///
    /// This is stored in each instance of the service, as it will be polled in
    /// `poll_ready` until a permit for the current request is acquired.
    ///
    /// A `ReusableBoxFuture` is used to avoid creating a new allocation every
    /// time a `Buffer` service has to acquire a permit. Instead, each clone of
    /// the `Buffer` holds a single allocation that is used every time a
    /// permit must be acquired.
    ///
    /// This future should only be polled when the `Buffer` is in the `Waiting`
    /// state. If the service is in the `Ready` or `Called` states, the future
    /// stored here will have already completed. When transitioning from
    /// `State::Called` to `State::Waiting`, a new future will be created.
    reserve: ReusableBoxFuture<Result<Permit<Request, T::Future>, SendError<()>>>,
    /// This buffer service's current state.
    ///
    /// The service either has acquired channel capacity and is ready for
    /// `call`; is waiting to acquire a permit and should continue polling
    /// `reserve`, or has consumed its permit in `call` and should reset
    /// `reserve` with a new future.
    state: State<Request, T::Future>,
    handle: Handle,
}

type Permit<R, F> = mpsc::OwnedPermit<Message<R, F>>;

enum State<R, F> {
    /// This service is waiting for channel capacity.
    ///
    /// The future in the `reserve` `ReusableBoxFuture` is able to be polled
    /// while in this state.
    Waiting,
    /// This service has acquired a channel permit and can now return `Ready`.
    ///
    /// The future in the `reserve` `ReusableBoxFuture` has completed if the
    /// service is in this state.
    Ready(Permit<R, F>),
    /// This service has consumed its channel permit and must now begin polling
    /// again.
    ///
    /// The future in the `reserve` `ReusableBoxFuture` has completed if the
    /// service is in this state.
    Called,
}

impl<T, Request> Buffer<T, Request>
where
    T: Service<Request>,
    T::Error: Into<crate::BoxError>,
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
    pub fn new(service: T, bound: usize) -> Self
    where
        T: Send + 'static,
        T::Future: Send,
        T::Error: Send + Sync,
        Request: Send + 'static,
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
    pub fn pair(service: T, bound: usize) -> (Buffer<T, Request>, Worker<T, Request>)
    where
        T: Send + 'static,
        T::Error: Send + Sync,
        Request: Send + 'static,
    {
        let (tx, rx) = mpsc::channel(bound);
        let (handle, worker) = Worker::new(service, rx);
        let buffer = Self::with_worker(handle, tx);
        (buffer, worker)
    }

    fn get_worker_error(&self) -> crate::BoxError {
        self.handle.get_error_on_closed()
    }
}

impl<T, Request> Buffer<T, Request>
where
    T: Service<Request>,
{
    fn with_worker(handle: Handle, tx: mpsc::Sender<Message<Request, T::Future>>) -> Self {
        // Create the `ReuseableBoxFuture` with a nop future, so that the new
        // service isn't queued to reserve capacity until `poll_ready` is called
        // for the first time.
        let reserve = ReusableBoxFuture::new(async move {
            unreachable!("because the service begins in the `Called` state, this future should never be polled")
        });

        Self {
            tx,
            reserve,
            handle,
            state: State::Called,
        }
    }
}

impl<T, Request> Service<Request> for Buffer<T, Request>
where
    T: Service<Request>,
    T::Error: Into<crate::BoxError>,
    T::Future: Send + 'static,
    Request: Send + 'static,
{
    type Response = T::Response;
    type Error = crate::BoxError;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        use State::*;

        // First, check if the worker is still alive.
        if self.tx.is_closed() {
            // If the inner service has errored, then we error here.
            return Poll::Ready(Err(self.get_worker_error()));
        }

        loop {
            match self.state {
                // If we've acquired capacity to send a request, we're ready!
                Ready(_) => return Poll::Ready(Ok(())),

                // If we haven't already acquired a permit, but have a `reserve`
                // future, poll the future to acquire one.
                Waiting => {
                    let permit =
                        ready!(self.reserve.poll(cx)).map_err(|_| self.get_worker_error())?;
                    self.state = State::Ready(permit);
                }

                // If we've completed the current `reserve` future and have
                // consumed the reserved permit, create a new `reserve` future.
                Called => {
                    self.reserve.set(self.tx.clone().reserve_owned());
                    self.state = State::Waiting;
                }
            }
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        tracing::trace!("sending request to buffer worker");
        let permit = match mem::replace(&mut self.state, State::Called) {
            State::Ready(permit) => permit,
            state => panic!(
                "buffer full; poll_ready must be called first (service was in the {:?} state)",
                state
            ),
        };

        // get the current Span so that we can explicitly propagate it to the worker
        // if we didn't do this, events on the worker related to this span wouldn't be counted
        // towards that span since the worker would have no way of entering it.
        let span = tracing::Span::current();

        // If we've made it here, then a semaphore permit has already been
        // acquired, so we can freely allocate a oneshot.
        let (tx, rx) = oneshot::channel();

        permit.send(Message { request, span, tx });
        ResponseFuture::new(rx)
    }
}

impl<T, Request> Clone for Buffer<T, Request>
where
    T: Service<Request>,
{
    fn clone(&self) -> Self {
        Self::with_worker(self.handle.clone(), self.tx.clone())
    }
}

// Custom `Debug` impl for `State` because the message and future types may not
// implement `Debug`.
impl<R, F> fmt::Debug for State<R, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use State::*;
        match self {
            Waiting => f.write_str("State::Waiting"),
            Ready(_) => f.write_str("State::Ready(...)"),
            Called => f.write_str("State::Called"),
        }
    }
}
