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

extern crate futures;
extern crate tower_service;

use futures::{Future, Stream, Poll, Async};
use futures::future::Executor;
use futures::sync::oneshot;
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};
use tower_service::Service;

use std::{error, fmt};
use std::sync::{Arc, RwLock};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::marker::PhantomData;


/// Adds a buffer in front of an inner service.
///
/// See crate level documentation for more details.
#[derive(Clone)]
pub struct Buffer<T, E>
where T: Service,
{
    tx: UnboundedSender<Message<T, E>>,
    state: Arc<State<T::Error>>,
}

/// Future eventually completed with the response to the original request.
pub struct ResponseFuture<T, E>
where T: Service,
{
    state: ResponseState<T::Future, E>,
    _err: PhantomData<E>,
}

/// Errors produced by `Buffer`.
#[derive(Debug)]
pub enum Error<E, C> {
    Inner(E),
    Closed(C),
}

/// Task that handles processing the buffer. This type should not be used
/// directly, instead `Buffer` requires an `Executor` that can accept this task.
pub struct Worker<T, E>
where T: Service,
{
    service: T,
    rx: UnboundedReceiver<Message<T, E>>,
    state: Arc<State<T::Error>>,
}

/// Error produced when spawning the worker fails
#[derive(Debug)]
pub struct SpawnError<T> {
    inner: T,
}

/// Message sent over buffer
#[derive(Debug)]
struct Message<T: Service, E> {
    request: T::Request,
    tx: oneshot::Sender<Result<T::Future, E>>,
}

/// State shared between `Buffer` and `Worker`
struct State<E> {
    open: AtomicBool,
    error: RwLock<Option<E>>,
}
enum ResponseState<T,E> {
    Rx(oneshot::Receiver<Result<T,E>>),
    Poll(T),
}

impl<T, E> Buffer<T, E>
where
    T: Service,
    E: Clone,
{
    /// Creates a new `Buffer` wrapping `service`.
    ///
    /// `executor` is used to spawn a new `Worker` task that is dedicated to
    /// draining the buffer and dispatching the requests to the internal
    /// service.
    pub fn new<X>(service: T, executor: &X) -> Result<Self, SpawnError<T>>
    where X: Executor<Worker<T, E>>,
    {
        let (tx, rx) = mpsc::unbounded();

        let state = Arc::new(State {
            open: AtomicBool::new(true),
            error: RwLock::new(None),
        });

        let worker = Worker {
            service,
            rx,
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

impl<T, E> Service for Buffer<T, E>
where
    T: Service,
    T::Error: Clone,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = Error<T::Error, E>;
    type Future = ResponseFuture<T, E>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // If the inner service has errored, then we error here.
        if !self.state.open.load(SeqCst) {
            let error = self.state.error
                .read().expect("worker thread panicked")
                .as_ref().map(Clone::clone)
                .expect("if the buffer is closed, the error should be set");
            return Err(Error::Inner(error));
        } else {
            // Ideally we could query if the `mpsc` is closed, but this is not
            // currently possible.
            Ok(().into())
        }
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        let (tx, rx) = oneshot::channel();

        // TODO: This could fail too
        let _ = self.tx.unbounded_send(Message {
            request,
            tx,
        });

        ResponseFuture { state: ResponseState::Rx(rx), _err: PhantomData }
    }
}

// ===== impl ResponseFuture =====

impl<T, E> Future for ResponseFuture<T, E>
where
    T: Service,
    T::Error: Clone,
{
    type Item = T::Response;
    type Error = Error<T::Error, E>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::ResponseState::*;

        loop {
            let fut;

            match self.state {
                Rx(ref mut rx) => {
                    match rx.poll() {
                        Ok(Async::Ready(Ok(f))) => fut = f,
                        Ok(Async::Ready(Err(e))) =>
                            // The buffer was closed by an error --- propagate
                            // it.
                            return Err(Error::Closed(e)),
                        Ok(Async::NotReady) =>
                            return Ok(Async::NotReady),
                        Err(_) => {
                            panic!("tx should not have been dropped on error!")
                        }
                    }
                }
                Poll(ref mut fut) => {
                    return fut.poll().map_err(|e| Error::Inner(e.clone()));
                }
            }

            self.state = Poll(fut);
        }
    }
}

// ===== impl Worker =====

impl<T, E> Future for Worker<T, E>
where
    T: Service,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        loop {
            // Wait for the service to be ready
            match self.service.poll_ready() {
                Ok(Async::Ready(_)) => {}
                Ok(Async::NotReady) => {
                    return Ok(Async::NotReady);
                }
                Err(e) => {
                    self.state.open.store(false, SeqCst);
                    let mut error = self.state.error.write()
                        // this shouldn't happen; other threads should only ever
                        // acquire read guards.
                        .expect("another thread acquired state write guard");
                    *error = Some(e);
                    return Ok(().into())
                }
            }

            // Get the next request
            match self.rx.poll() {
                Ok(Async::Ready(Some(Message { request, tx }))) => {
                    // Received a request. Dispatch the request to the inner
                    // service and get the response future
                    let response = self.service.call(request);

                    // Send the response future back to the sender.
                    let _ = tx.send(Ok(response));
                }
                Ok(Async::Ready(None)) => {
                    // All senders are dropped... the task is no longer needed
                    return Ok(().into());
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(()) => unreachable!(),
            }
        }
    }
}

// ===== impl Error =====

impl<E, C> fmt::Display for Error<E, C>
where
    E: fmt::Display,
    C: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Inner(ref why) => fmt::Display::fmt(why, f),
            Error::Closed(ref why) => write!(f, "buffer closed: {}", why),
        }
    }
}

impl<E, C> error::Error for Error<E, C>
where
    E: error::Error,
    C: error::Error,
{
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Inner(ref why) => Some(why),
            Error::Closed(ref why) => Some(why),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Inner(_) => "inner service error",
            Error::Closed(_) => "buffer closed",
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
