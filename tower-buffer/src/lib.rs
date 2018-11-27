//! Buffer requests when the inner service is out of capacity.
//!
//! Buffering works by spawning a new task that is dedicated to pulling requests
//! out of the buffer and dispatching them to the inner service. By adding a
//! buffer and a dedicated task, the `Buffer` layer in front of the service can
//! be `Clone` even if the inner service is not.
//!
//! Currently, `Buffer` uses an unbounded buffering strategy, which is not a
//! good thing to put in production situations. However, it illustrates the idea
//! and capabilities around adding buffering to an arbitrary `Service`.

#[macro_use]
extern crate futures;
extern crate tower_service;
extern crate tokio_executor;
extern crate tower_direct_service;

use futures::future::Executor;
use futures::sync::oneshot;
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};
use futures::{Async, Future, Poll, Stream};
use std::marker::PhantomData;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::{error, fmt};
use tower_service::Service;
use tokio_executor::DefaultExecutor;
use tower_direct_service::DirectService;

/// Adds a buffer in front of an inner service.
///
/// See crate level documentation for more details.
pub struct Buffer<T, Request>
where
    T: Service<Request>,
{
    tx: UnboundedSender<Message<Request, T::Future>>,
    state: Arc<State>,
}

/// A [`Buffer`] that is backed by a `DirectService`.
pub type DirectBuffer<T, Request> = Buffer<DirectServiceRef<T>, Request>;

/// Future eventually completed with the response to the original request.
pub struct ResponseFuture<T> {
    state: ResponseState<T>,
}

/// Errors produced by `Buffer`.
#[derive(Debug)]
pub enum Error<T> {
    /// The `Service` call errored.
    Inner(T),
    /// The underlying `Service` failed.
    Closed,
}

/// An adapter that exposes the associated types of a `DirectService` through `Service`.
/// This type does *not* let you pretend that a `DirectService` is a `Service`; that would be
/// incorrect, as the caller would then not call `poll_service` and `poll_close` as necessary on
/// the underlying `DirectService`. Instead, it merely provides a type-level adapter which allows
/// types that are generic over `T: Service`, but only need access to associated types of `T`, to
/// also take a `DirectService` ([`Buffer`] is an example of such a type).
pub struct DirectServiceRef<T> {
    _marker: PhantomData<T>,
}

impl<T, Request> Service<Request> for DirectServiceRef<T>
where
    T: DirectService<Request>,
{
    type Response = T::Response;
    type Error = T::Error;
    type Future = T::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        unreachable!("tried to poll a DirectService through a marker reference")
    }

    fn call(&mut self, _: Request) -> Self::Future {
        unreachable!("tried to call a DirectService through a marker reference")
    }
}

/// A wrapper that exposes a `Service` (which does not need to be driven) as a `DirectService` so
/// that a construct that is *able* to take a `DirectService` can also take instances of
/// `Service`.
pub struct DirectedService<T>(T);

impl<T, Request> DirectService<Request> for DirectedService<T>
where
    T: Service<Request>,
{
    type Response = T::Response;
    type Error = T::Error;
    type Future = T::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.0.poll_ready()
    }

    fn poll_service(&mut self) -> Poll<(), Self::Error> {
        // TODO: is this the right thing to do?
        Ok(Async::Ready(()))
    }

    fn poll_close(&mut self) -> Poll<(), Self::Error> {
        // TODO: is this the right thing to do?
        Ok(Async::Ready(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        self.0.call(req)
    }
}

mod sealed {
    use super::*;

    /// Task that handles processing the buffer. This type should not be used
    /// directly, instead `Buffer` requires an `Executor` that can accept this task.
    pub struct Worker<T, Request>
    where
        T: DirectService<Request>,
    {
        pub(crate) current_message: Option<Message<Request, T::Future>>,
        pub(crate) rx: UnboundedReceiver<Message<Request, T::Future>>,
        pub(crate) service: T,
        pub(crate) finish: bool,
        pub(crate) state: Arc<State>,
    }
}
use sealed::Worker;

/// This trait allows you to use either Tokio's threaded runtime's executor or the `current_thread`
/// runtime's executor depending on if `T` is `Send` or `!Send`.
pub trait WorkerExecutor<T, Request>: Executor<sealed::Worker<T, Request>>
where
    T: DirectService<Request>,
{
}

impl<T, Request, E: Executor<sealed::Worker<T, Request>>> WorkerExecutor<T, Request> for E where
    T: DirectService<Request>
{
}

/// Error produced when spawning the worker fails
#[derive(Debug)]
pub struct SpawnError<T> {
    inner: T,
}

/// Message sent over buffer
#[derive(Debug)]
struct Message<Request, Fut> {
    request: Request,
    tx: oneshot::Sender<Fut>,
}

/// State shared between `Buffer` and `Worker`
struct State {
    open: AtomicBool,
}

enum ResponseState<T> {
    Rx(oneshot::Receiver<T>),
    Poll(T),
}

impl<T, Request> Buffer<T, Request>
where
    T: Service<Request>,
{
    /// Creates a new `Buffer` wrapping `service`.
    ///
    /// The default Tokio executor is used to run the given service, which means that this method
    /// must be called while on the Tokio runtime.
    pub fn new(service: T) -> Result<Self, SpawnError<T>>
    where
        T: Send + 'static,
        T::Future: Send,
        Request: Send + 'static,
    {
        Self::with_executor(service, &DefaultExecutor::current())
    }

    /// Creates a new `Buffer` wrapping `service`.
    ///
    /// `executor` is used to spawn a new `Worker` task that is dedicated to
    /// draining the buffer and dispatching the requests to the internal
    /// service.
    pub fn with_executor<E>(service: T, executor: &E) -> Result<Self, SpawnError<T>>
    where
        E: WorkerExecutor<DirectedService<T>, Request>,
    {
        let (tx, rx) = mpsc::unbounded();

        let state = Arc::new(State {
            open: AtomicBool::new(true),
        });

        let worker = Worker {
            current_message: None,
            rx,
            service: DirectedService(service),
            finish: false,
            state: state.clone(),
        };

        // TODO: handle error
        executor.execute(worker)
            .ok().unwrap();

        Ok(Buffer {
            tx,
            state: state,
        })
    }
}

impl<T, Request> Buffer<DirectServiceRef<T>, Request>
where
    T: DirectService<Request>,
{
    /// Creates a new `Buffer` wrapping the given directly driven `service`.
    ///
    /// `executor` is used to spawn a new `Worker` task that is dedicated to
    /// draining the buffer and dispatching the requests to the internal
    /// service.
    pub fn new_direct<E>(service: T, executor: &E) -> Result<Self, SpawnError<T>>
    where
        E: Executor<Worker<T, Request>>,
    {
        let (tx, rx) = mpsc::unbounded();

        let state = Arc::new(State {
            open: AtomicBool::new(true),
        });

        let worker = Worker {
            current_message: None,
            rx,
            service,
            finish: false,
            state: state.clone(),
        };

        // TODO: handle error
        executor.execute(worker).ok().unwrap();

        Ok(Buffer { tx, state: state })
    }
}

impl<T, Request> Service<Request> for Buffer<T, Request>
where
    T: Service<Request>,
{
    type Response = T::Response;
    type Error = Error<T::Error>;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // If the inner service has errored, then we error here.
        if !self.state.open.load(Ordering::Acquire) {
            return Err(Error::Closed);
        } else {
            // Ideally we could query if the `mpsc` is closed, but this is not
            // currently possible.
            Ok(().into())
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let (tx, rx) = oneshot::channel();

        let sent = self.tx.unbounded_send(Message {
            request,
            tx,
        });

        if sent.is_err() {
            self.state.open.store(false, Ordering::Release);
        }

        ResponseFuture { state: ResponseState::Rx(rx) }
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

impl<T> Future for ResponseFuture<T>
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
                Rx(ref mut rx) => {
                    match rx.poll() {
                        Ok(Async::Ready(f)) => fut = f,
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(_) => return Err(Error::Closed),
                    }
                }
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
    T: DirectService<Request>,
{
    /// Return the next queued Message that hasn't been canceled.
    fn poll_next_msg(&mut self) -> Poll<Option<Message<Request, T::Future>>, ()> {
        if self.finish {
            // We've already received None and are shutting down
            return Ok(Async::Ready(None));
        }

        if let Some(mut msg) = self.current_message.take() {
            // poll_cancel returns Async::Ready is the receiver is dropped.
            // Returning NotReady means it is still alive, so we should still
            // use it.
            if msg.tx.poll_cancel()?.is_not_ready() {
                return Ok(Async::Ready(Some(msg)));
            }
        }

        // Get the next request
        while let Some(mut msg) = try_ready!(self.rx.poll()) {
            if msg.tx.poll_cancel()?.is_not_ready() {
                return Ok(Async::Ready(Some(msg)));
            }
            // Otherwise, request is canceled, so pop the next one.
        }

        Ok(Async::Ready(None))
    }
}

impl<T, Request> Future for Worker<T, Request>
where
    T: DirectService<Request>,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        let mut any_outstanding = true;
        loop {
            match self.poll_next_msg()? {
                Async::Ready(Some(msg)) => {
                    // Wait for the service to be ready
                    match self.service.poll_ready() {
                        Ok(Async::Ready(())) => {
                            let response = self.service.call(msg.request);

                            // Send the response future back to the sender.
                            //
                            // An error means the request had been canceled in-between
                            // our calls, the response future will just be dropped.
                            let _ = msg.tx.send(response);

                            // Try to queue another request before we poll outstanding requests.
                            any_outstanding = true;
                            continue;
                        }
                        Ok(Async::NotReady) => {
                            // Put out current message back in its slot.
                            self.current_message = Some(msg);
                            // We don't want to return quite yet
                            // We want to also make progress on current requests
                            break;
                        }
                        Err(_) => {
                            self.state.open.store(false, Ordering::Release);
                            return Ok(().into());
                        }
                    }
                }
                Async::Ready(None) => {
                    // No more more requests _ever_.
                    self.finish = true;
                }
                Async::NotReady if any_outstanding => {
                    // Make some progress on the service if we can.
                }
                Async::NotReady => {
                    // There are no outstanding requests to make progress on.
                    // And we don't have any new requests to enqueue.
                    // So we yield.
                    return Ok(Async::NotReady);
                }
            }

            if self.finish {
                try_ready!(self.service.poll_close().map_err(|_| ()));
                // We are all done!
                break;
            } else {
                if let Async::Ready(()) = self.service.poll_service().map_err(|_| ())? {
                    // Note to future iterations that there's no reason to call poll_service.
                    any_outstanding = false;
                }
            }
        }

        // All senders are dropped... the task is no longer needed
        Ok(().into())
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
            Error::Closed => f.pad("buffer closed"),
        }
    }
}

impl<T> error::Error for Error<T>
where
    T: error::Error,
{
    fn cause(&self) -> Option<&error::Error> {
        if let Error::Inner(ref why) = *self {
            Some(why)
        } else {
            None
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Inner(ref e) => e.description(),
            Error::Closed => "buffer closed",
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
