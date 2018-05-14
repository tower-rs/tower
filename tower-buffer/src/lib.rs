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

use futures::{Future, Stream, Poll, Async};
use futures::future::Executor;
use futures::sync::oneshot;
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};
use tower_service::Service;

use std::{error, fmt};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

/// Adds a buffer in front of an inner service.
///
/// See crate level documentation for more details.
#[derive(Clone)]
pub struct Buffer<T>
where T: Service,
{
    tx: UnboundedSender<Message<T>>,
    state: Arc<State>,
}

/// Future eventually completed with the response to the original request.
pub struct ResponseFuture<T>
where T: Service,
{
    state: ResponseState<T::Future>,
}

/// Errors produced by `Buffer`.
#[derive(Debug)]
pub enum Error<T> {
    Inner(T),
    Closed,
}

/// Task that handles processing the buffer. This type should not be used
/// directly, instead `Buffer` requires an `Executor` that can accept this task.
pub struct Worker<T>
where T: Service,
{
    current_message: Option<Message<T>>,
    rx: UnboundedReceiver<Message<T>>,
    service: T,
    state: Arc<State>,
}

/// Error produced when spawning the worker fails
#[derive(Debug)]
pub struct SpawnError<T> {
    inner: T,
}

/// Message sent over buffer
#[derive(Debug)]
struct Message<T: Service> {
    request: T::Request,
    tx: oneshot::Sender<T::Future>,
}

/// State shared between `Buffer` and `Worker`
struct State {
    open: AtomicBool,
}

enum ResponseState<T> {
    Rx(oneshot::Receiver<T>),
    Poll(T),
}

impl<T> Buffer<T>
where T: Service,
{
    /// Creates a new `Buffer` wrapping `service`.
    ///
    /// `executor` is used to spawn a new `Worker` task that is dedicated to
    /// draining the buffer and dispatching the requests to the internal
    /// service.
    pub fn new<E>(service: T, executor: &E) -> Result<Self, SpawnError<T>>
    where E: Executor<Worker<T>>,
    {
        let (tx, rx) = mpsc::unbounded();

        let state = Arc::new(State {
            open: AtomicBool::new(true),
        });

        let worker = Worker {
            current_message: None,
            rx,
            service,
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

impl<T> Service for Buffer<T>
where T: Service,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = Error<T::Error>;
    type Future = ResponseFuture<T>;

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

    fn call(&mut self, request: Self::Request) -> Self::Future {
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

// ===== impl ResponseFuture =====

impl<T> Future for ResponseFuture<T>
where T: Service
{
    type Item = T::Response;
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

impl<T> Worker<T>
where T: Service
{
    /// Return the next queued Message that hasn't been canceled.
    fn poll_next_msg(&mut self) -> Poll<Option<Message<T>>, ()> {
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

impl<T> Future for Worker<T>
where T: Service,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        while let Some(msg) = try_ready!(self.poll_next_msg()) {
            // Wait for the service to be ready
            match self.service.poll_ready() {
                Ok(Async::Ready(())) => {
                    let response = self.service.call(msg.request);

                    // Send the response future back to the sender.
                    //
                    // An error means the request had been canceled in-between
                    // our calls, the response future will just be dropped.
                    let _ = msg.tx.send(response);
                }
                Ok(Async::NotReady) => {
                    // Put out current message back in its slot.
                    self.current_message = Some(msg);
                    return Ok(Async::NotReady);
                }
                Err(_) => {
                    self.state.open.store(false, Ordering::Release);
                    return Ok(().into())
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
