//! Mock `Service` that can be used in tests.

extern crate tower_service;
extern crate futures;

use tower_service::Service;

use futures::{Future, Stream, Poll, Async};
use futures::sync::{oneshot, mpsc};
use futures::task::{self, Task};

use std::{ops, u64};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A mock service
#[derive(Debug)]
pub struct Mock<T, U, E> {
    id: u64,
    tx: Mutex<Tx<T, U, E>>,
    state: Arc<Mutex<State>>,
    can_send: bool,
}

/// Handle to the `Mock`.
#[derive(Debug)]
pub struct Handle<T, U, E> {
    rx: Rx<T, U, E>,
    state: Arc<Mutex<State>>,
}

#[derive(Debug)]
pub struct Request<T, U, E> {
    request: T,
    respond: Respond<U, E>,
}

/// Respond to a request received by `Mock`.
#[derive(Debug)]
pub struct Respond<T, E> {
    tx: oneshot::Sender<Result<T, E>>,
}

/// Future of the `Mock` response.
#[derive(Debug)]
pub struct ResponseFuture<T, E> {
    // Slight abuse of the error enum...
    rx: Error<oneshot::Receiver<Result<T, E>>>,
}

/// Enumeration of errors that can be returned by `Mock`.
#[derive(Debug, PartialEq)]
pub enum Error<T> {
    Closed,
    NoCapacity,
    Other(T),
}

#[derive(Debug)]
struct State {
    // Tracks the number of requests that can be sent through
    rem: u64,

    // Tasks that are blocked
    tasks: HashMap<u64, Task>,

    // Tracks if the `Handle` dropped
    is_closed: bool,

    // Tracks the ID for the next mock clone
    next_clone_id: u64,
}

type Tx<T, U, E> = mpsc::UnboundedSender<Request<T, U, E>>;
type Rx<T, U, E> = mpsc::UnboundedReceiver<Request<T, U, E>>;

// ===== impl Mock =====

impl<T, U, E> Mock<T, U, E> {
    /// Create a new `Mock` and `Handle` pair.
    pub fn new() -> (Self, Handle<T, U, E>) {
        let (tx, rx) = mpsc::unbounded();
        let tx = Mutex::new(tx);

        let state = Arc::new(Mutex::new(State::new()));

        let mock = Mock {
            id: 0,
            tx,
            state: state.clone(),
            can_send: false,
        };

        let handle = Handle {
            rx,
            state,
        };

        (mock, handle)
    }
}

impl<T, U, E> Service<T> for Mock<T, U, E> {
    type Response = U;
    type Error = Error<E>;
    type Future = ResponseFuture<U, E>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let mut state = self.state.lock().unwrap();

        if state.is_closed {
            return Err(Error::Closed);
        }

        if self.can_send {
            return Ok(().into());
        }

        if state.rem > 0 {
            assert!(!state.tasks.contains_key(&self.id));

            // Returning `Ready` means the next call to `call` must succeed.
            self.can_send = true;

            Ok(Async::Ready(()))
        } else {
            // Bit weird... but whatevz
            *state.tasks.entry(self.id)
                .or_insert_with(|| task::current()) = task::current();

            Ok(Async::NotReady)
        }
    }

    fn call(&mut self, request: T) -> Self::Future {
        // Make sure that the service has capacity
        let mut state = self.state.lock().unwrap();

        if state.is_closed {
            return ResponseFuture {
                rx: Error::Closed,
            };
        }

        if !self.can_send {
            if state.rem == 0 {
                return ResponseFuture {
                    rx: Error::NoCapacity,
                }
            }
        }

        self.can_send = false;

        // Decrement the number of remaining requests that can be sent
        if state.rem > 0 {
            state.rem -= 1;
        }

        let (tx, rx) = oneshot::channel();

        let request = Request {
            request,
            respond: Respond { tx },
        };

        match self.tx.lock().unwrap().unbounded_send(request) {
            Ok(_) => {}
            Err(_) => {
                // TODO: Can this be reached
                return ResponseFuture {
                    rx: Error::Closed,
                };
            }
        }

        ResponseFuture { rx: Error::Other(rx) }
    }
}

impl<T, U, E> Clone for Mock<T, U, E> {
    fn clone(&self) -> Self {
        let id = {
            let mut state = self.state.lock().unwrap();
            let id = state.next_clone_id;

            state.next_clone_id += 1;

            id
        };

        let tx = Mutex::new(self.tx.lock().unwrap().clone());

        Mock {
            id,
            tx,
            state: self.state.clone(),
            can_send: false,
        }
    }
}

impl<T, U, E> Drop for Mock<T, U, E> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.tasks.remove(&self.id);
    }
}

// ===== impl Handle =====

impl<T, U, E> Handle<T, U, E> {
    /// Asynchronously gets the next request
    pub fn poll_request(&mut self)
        -> Poll<Option<Request<T, U, E>>, ()>
    {
        self.rx.poll()
    }

    /// Synchronously gets the next request.
    ///
    /// This function blocks the current thread until a request is received.
    pub fn next_request(&mut self) -> Option<Request<T, U, E>> {
        use futures::future::poll_fn;
        poll_fn(|| self.poll_request()).wait().unwrap()
    }

    /// Allow a certain number of requests
    pub fn allow(&mut self, num: u64) {
        let mut state = self.state.lock().unwrap();
        state.rem = num;

        if num > 0 {
            for (_, task) in state.tasks.drain() {
                task.notify();
            }
        }
    }
}

impl<T, U, E> Drop for Handle<T, U, E> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.is_closed = true;

        for (_, task) in state.tasks.drain() {
            task.notify();
        }
    }
}

// ===== impl Request =====

impl<T, U, E> Request<T, U, E> {
    /// Split the request and respond handle
    pub fn into_parts(self) -> (T, Respond<U, E>) {
        (self.request, self.respond)
    }

    pub fn respond(self, response: U) {
        self.respond.respond(response)
    }

    pub fn error(self, err: E) {
        self.respond.error(err)
    }
}

impl<T, U, E> ops::Deref for Request<T, U, E> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.request
    }
}

// ===== impl Respond =====

impl<T, E> Respond<T, E> {
    pub fn respond(self, response: T) {
        // TODO: Should the result be dropped?
        let _ = self.tx.send(Ok(response));
    }

    pub fn error(self, err: E) {
        // TODO: Should the result be dropped?
        let _ = self.tx.send(Err(err));
    }
}

// ===== impl ResponseFuture =====

impl<T, E> Future for ResponseFuture<T, E> {
    type Item = T;
    type Error = Error<E>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.rx {
            Error::Other(ref mut rx) => {
                match rx.poll() {
                    Ok(Async::Ready(Ok(v))) => Ok(v.into()),
                    Ok(Async::Ready(Err(e))) => Err(Error::Other(e)),
                    Ok(Async::NotReady) => Ok(Async::NotReady),
                    Err(_) => Err(Error::Closed),
                }
            }
            Error::NoCapacity => Err(Error::NoCapacity),
            Error::Closed => Err(Error::Closed),
        }
    }
}

// ===== impl State =====

impl State {
    fn new() -> State {
        State {
            rem: u64::MAX,
            tasks: HashMap::new(),
            is_closed: false,
            next_clone_id: 1,
        }
    }
}
