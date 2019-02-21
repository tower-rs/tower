//! Buffer requests when the inner service is out of capacity.
//!
//! Buffering works by spawning a new task that is dedicated to pulling requests
//! out of the buffer and dispatching them to the inner service. By adding a
//! buffer and a dedicated task, the `Buffer` layer in front of the service can
//! be `Clone` even if the inner service is not.

#[macro_use]
extern crate futures;
extern crate lazycell;
extern crate tokio_executor;
extern crate tokio_sync;
extern crate tower_service;

use futures::future::Executor;
use futures::{Async, Future, Poll, Stream};
use std::sync::Arc;
use std::{error, fmt};
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
    state: Arc<State<T::Error>>,
}

/// Future eventually completed with the response to the original request.
pub struct ResponseFuture<T, E> {
    state: ResponseState<T, E>,
}

/// An error produced by a `Service` wrapped by a `Buffer`
#[derive(Debug)]
pub struct ServiceError<E> {
    method: &'static str,
    inner: E,
}

impl<E> ServiceError<E> {
    /// The error produced by the `Service` when `method` was called.
    pub fn error(&self) -> &E {
        &self.inner
    }
}

/// Errors produced by `Buffer`.
#[derive(Debug)]
pub enum Error<E> {
    /// The `Service` call errored.
    Inner(E),
    /// The underlying `Service` failed. All subsequent requests will fail.
    Closed(Arc<ServiceError<E>>),
}

mod sealed {
    use super::*;

    /// Task that handles processing the buffer. This type should not be used
    /// directly, instead `Buffer` requires an `Executor` that can accept this task.
    pub struct Worker<T, Request>
    where
        T: Service<Request>,
    {
        pub(crate) current_message: Option<Message<Request, T::Future, T::Error>>,
        pub(crate) rx: mpsc::Receiver<Message<Request, T::Future, T::Error>>,
        pub(crate) service: T,
        pub(crate) finish: bool,
        pub(crate) failed: Option<Arc<ServiceError<T::Error>>>,
        pub(crate) state: Arc<State<T::Error>>,
    }
}
use sealed::Worker;

/// This trait allows you to use either Tokio's threaded runtime's executor or the `current_thread`
/// runtime's executor depending on if `T` is `Send` or `!Send`.
pub trait WorkerExecutor<T, Request>: Executor<sealed::Worker<T, Request>>
where
    T: Service<Request>,
{
}

impl<T, Request, E: Executor<sealed::Worker<T, Request>>> WorkerExecutor<T, Request> for E where
    T: Service<Request>
{
}

/// Error produced when spawning the worker fails
#[derive(Debug)]
pub struct SpawnError<T> {
    inner: T,
}

/// Message sent over buffer
#[derive(Debug)]
struct Message<Request, Fut, E> {
    request: Request,
    tx: oneshot::Sender<Result<Fut, Arc<ServiceError<E>>>>,
}

/// State shared between `Buffer` and `Worker`
struct State<E> {
    err: lazycell::AtomicLazyCell<Arc<ServiceError<E>>>,
}

enum ResponseState<T, E> {
    Failed(Arc<ServiceError<E>>),
    Rx(oneshot::Receiver<Result<T, Arc<ServiceError<E>>>>),
    Poll(T),
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

        let state = Arc::new(State {
            err: lazycell::AtomicLazyCell::new(),
        });

        match Worker::spawn(service, rx, state.clone(), executor) {
            Ok(()) => Ok(Buffer { tx, state: state }),
            Err(service) => Err(SpawnError { inner: service }),
        }
    }

    fn get_error_on_closed(&self) -> Arc<ServiceError<T::Error>> {
        self.state
            .err
            .borrow()
            .cloned()
            .expect("Worker exited, but did not set error.")
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
            .map_err(move |_| Error::Closed(self.get_error_on_closed()))
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
                    ResponseFuture {
                        state: ResponseState::Failed(self.get_error_on_closed()),
                    }
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
            Ok(_) => ResponseFuture {
                state: ResponseState::Rx(rx),
            },
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
            state: self.state.clone(),
        }
    }
}

// ===== impl ResponseFuture =====

impl<T> Future for ResponseFuture<T, T::Error>
where
    T: Future,
{
    type Item = T::Item;
    type Error = Error<T::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::ResponseState::*;

        loop {
            let fut;

            match self.state {
                Failed(ref e) => {
                    return Err(Error::Closed(e.clone()));
                }
                Rx(ref mut rx) => match rx.poll() {
                    Ok(Async::Ready(Ok(f))) => fut = f,
                    Ok(Async::Ready(Err(e))) => return Err(Error::Closed(e)),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(_) => unreachable!(
                        "Worker exited without sending error to all outstanding requests."
                    ),
                },
                Poll(ref mut fut) => {
                    return fut.poll().map_err(Error::Inner);
                }
            }

            self.state = Poll(fut);
        }
    }
}

// ===== impl Worker =====

impl<T, Request> Worker<T, Request>
where
    T: Service<Request>,
{
    fn spawn<E>(
        service: T,
        rx: mpsc::Receiver<Message<Request, T::Future, T::Error>>,
        state: Arc<State<T::Error>>,
        executor: &E,
    ) -> Result<(), T>
    where
        E: WorkerExecutor<T, Request>,
    {
        let worker = Worker {
            current_message: None,
            finish: false,
            failed: None,
            rx,
            service,
            state,
        };

        match executor.execute(worker) {
            Ok(()) => Ok(()),
            Err(err) => Err(err.into_future().service),
        }
    }
}

impl<T, Request> Worker<T, Request>
where
    T: Service<Request>,
{
    /// Return the next queued Message that hasn't been canceled.
    fn poll_next_msg(&mut self) -> Poll<Option<Message<Request, T::Future, T::Error>>, ()> {
        if self.finish {
            // We've already received None and are shutting down
            return Ok(Async::Ready(None));
        }

        if let Some(mut msg) = self.current_message.take() {
            // poll_cancel returns Async::Ready is the receiver is dropped.
            // Returning NotReady means it is still alive, so we should still
            // use it.
            if msg.tx.poll_close()?.is_not_ready() {
                return Ok(Async::Ready(Some(msg)));
            }
        }

        // Get the next request
        while let Some(mut msg) = try_ready!(self.rx.poll().map_err(|_| ())) {
            if msg.tx.poll_close()?.is_not_ready() {
                return Ok(Async::Ready(Some(msg)));
            }
            // Otherwise, request is canceled, so pop the next one.
        }

        Ok(Async::Ready(None))
    }

    fn failed(&mut self, method: &'static str, error: T::Error) {
        // The underlying service failed when we called `method` on it with the given `error`. We
        // need to communicate this to all the `Buffer` handles. To do so, we wrap up the error in
        // an `Arc`, send that `Arc<E>` to all pending requests, and store it so that subsequent
        // requests will also fail with the same error.

        // Note that we need to handle the case where some handle is concurrently trying to send us
        // a request. We need to make sure that *either* the send of the request fails *or* it
        // receives an error on the `oneshot` it constructed. Specifically, we want to avoid the
        // case where we send errors to all outstanding requests, and *then* the caller sends its
        // request. We do this by *first* exposing the error, *then* closing the channel used to
        // send more requests (so the client will see the error when the send fails), and *then*
        // sending the error to all outstanding requests.
        let error = Arc::new(ServiceError {
            method,
            inner: error,
        });
        if let Err(_) = self.state.err.fill(error.clone()) {
            // Future::poll was called after we've already errored out!
            return;
        }
        self.rx.close();

        // By closing the mpsc::Receiver, we know that poll_next_msg will soon return Ready(None),
        // which will trigger the `self.finish == true` phase. We just need to make sure that any
        // requests that we receive before we've exhausted the receiver receive the error:
        self.failed = Some(error);
    }
}

impl<T, Request> Future for Worker<T, Request>
where
    T: Service<Request>,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        if self.finish {
            return Ok(().into());
        }

        loop {
            match self.poll_next_msg()? {
                Async::Ready(Some(msg)) => {
                    if let Some(ref failed) = self.failed {
                        let _ = msg.tx.send(Err(failed.clone()));
                        continue;
                    }

                    // Wait for the service to be ready
                    match self.service.poll_ready() {
                        Ok(Async::Ready(())) => {
                            let response = self.service.call(msg.request);

                            // Send the response future back to the sender.
                            //
                            // An error means the request had been canceled in-between
                            // our calls, the response future will just be dropped.
                            let _ = msg.tx.send(Ok(response));
                            continue;
                        }
                        Ok(Async::NotReady) => {
                            // Put out current message back in its slot.
                            self.current_message = Some(msg);
                            return Ok(Async::NotReady);
                        }
                        Err(e) => {
                            self.failed("poll_ready", e);
                            let _ = msg.tx.send(Err(self
                                .failed
                                .clone()
                                .expect("Worker::failed did not set self.failed?")));
                        }
                    }
                }
                Async::Ready(None) => {
                    // No more more requests _ever_.
                    self.finish = true;
                    return Ok(Async::Ready(()));
                }
                Async::NotReady if self.failed.is_some() => {
                    // No need to poll the service as it has already failed.
                    return Ok(Async::NotReady);
                }
                Async::NotReady => {
                    // We don't have any new requests to enqueue.
                    // So we yield.
                    return Ok(Async::NotReady);
                }
            }
        }
    }
}

// ===== impl Error =====

impl<T> fmt::Display for Error<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Inner(ref why) => fmt::Display::fmt(why, f),
            Error::Closed(ref e) => write!(f, "Service::{} failed: {}", e.method, e.inner),
        }
    }
}

impl<T> error::Error for Error<T>
where
    T: error::Error,
{
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Inner(ref why) => Some(why),
            Error::Closed(ref e) => Some(&e.inner),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Inner(ref e) => e.description(),
            Error::Closed(ref e) => e.inner.description(),
        }
    }
}

// ===== impl SpawnError =====

impl<T> fmt::Display for SpawnError<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error spawning buffer task: {:?}", self.inner)
    }
}

impl<T> error::Error for SpawnError<T>
where
    T: error::Error,
{
    fn cause(&self) -> Option<&error::Error> {
        Some(&self.inner)
    }

    fn description(&self) -> &str {
        "error spawning buffer task"
    }
}
