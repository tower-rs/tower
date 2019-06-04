//! Mock `Service` that can be used in tests.

pub mod error;
pub mod future;

use crate::mock::{error::Error, future::ResponseFuture};
use futures::{
    task::{self, Task},
    Async, Future, Poll, Stream,
};
use tokio_sync::{mpsc, oneshot};
use tower_service::Service;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    u64,
};

/// A mock service
#[derive(Debug)]
pub struct Mock<T, U> {
    id: u64,
    tx: Mutex<Tx<T, U>>,
    state: Arc<Mutex<State>>,
    can_send: bool,
}

/// Handle to the `Mock`.
#[derive(Debug)]
pub struct Handle<T, U> {
    rx: Rx<T, U>,
    state: Arc<Mutex<State>>,
}

type Request<T, U> = (T, SendResponse<U>);

/// Send a response in reply to a received request.
#[derive(Debug)]
pub struct SendResponse<T> {
    tx: oneshot::Sender<Result<T, Error>>,
}

#[derive(Debug)]
struct State {
    /// Tracks the number of requests that can be sent through
    rem: u64,

    /// Tasks that are blocked
    tasks: HashMap<u64, Task>,

    /// Tracks if the `Handle` dropped
    is_closed: bool,

    /// Tracks the ID for the next mock clone
    next_clone_id: u64,

    /// Tracks the next error to yield (if any)
    err_with: Option<Error>,
}

type Tx<T, U> = mpsc::UnboundedSender<Request<T, U>>;
type Rx<T, U> = mpsc::UnboundedReceiver<Request<T, U>>;

/// Create a new `Mock` and `Handle` pair.
pub fn pair<T, U>() -> (Mock<T, U>, Handle<T, U>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let tx = Mutex::new(tx);

    let state = Arc::new(Mutex::new(State::new()));

    let mock = Mock {
        id: 0,
        tx,
        state: state.clone(),
        can_send: false,
    };

    let handle = Handle { rx, state };

    (mock, handle)
}

impl<T, U> Service<T> for Mock<T, U> {
    type Response = U;
    type Error = Error;
    type Future = ResponseFuture<U>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let mut state = self.state.lock().unwrap();

        if state.is_closed {
            return Err(error::Closed::new().into());
        }

        if let Some(e) = state.err_with.take() {
            return Err(e);
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
            *state
                .tasks
                .entry(self.id)
                .or_insert_with(|| task::current()) = task::current();

            Ok(Async::NotReady)
        }
    }

    fn call(&mut self, request: T) -> Self::Future {
        // Make sure that the service has capacity
        let mut state = self.state.lock().unwrap();

        if state.is_closed {
            return ResponseFuture::closed();
        }

        if !self.can_send {
            panic!("service not ready; poll_ready must be called first");
        }

        self.can_send = false;

        // Decrement the number of remaining requests that can be sent
        if state.rem > 0 {
            state.rem -= 1;
        }

        let (tx, rx) = oneshot::channel();
        let send_response = SendResponse { tx };

        match self.tx.lock().unwrap().try_send((request, send_response)) {
            Ok(_) => {}
            Err(_) => {
                // TODO: Can this be reached
                return ResponseFuture::closed();
            }
        }

        ResponseFuture::new(rx)
    }
}

impl<T, U> Clone for Mock<T, U> {
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

impl<T, U> Drop for Mock<T, U> {
    fn drop(&mut self) {
        let mut state = match self.state.lock() {
            Ok(v) => v,
            Err(e) => {
                if ::std::thread::panicking() {
                    return;
                }

                panic!("{:?}", e);
            }
        };

        state.tasks.remove(&self.id);
    }
}

// ===== impl Handle =====

impl<T, U> Handle<T, U> {
    /// Asynchronously gets the next request
    pub fn poll_request(&mut self) -> Poll<Option<Request<T, U>>, Error> {
        self.rx.poll().map_err(Into::into)
    }

    /// Synchronously gets the next request.
    ///
    /// This function blocks the current thread until a request is received.
    pub fn next_request(&mut self) -> Option<Request<T, U>> {
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

    /// Make the next poll_ method error with the given error.
    pub fn send_error<E: Into<Error>>(&mut self, e: E) {
        let mut state = self.state.lock().unwrap();
        state.err_with = Some(e.into());

        for (_, task) in state.tasks.drain() {
            task.notify();
        }
    }
}

impl<T, U> Drop for Handle<T, U> {
    fn drop(&mut self) {
        let mut state = match self.state.lock() {
            Ok(v) => v,
            Err(e) => {
                if ::std::thread::panicking() {
                    return;
                }

                panic!("{:?}", e);
            }
        };

        state.is_closed = true;

        for (_, task) in state.tasks.drain() {
            task.notify();
        }
    }
}

// ===== impl SendResponse =====

impl<T> SendResponse<T> {
    pub fn send_response(self, response: T) {
        // TODO: Should the result be dropped?
        let _ = self.tx.send(Ok(response));
    }

    pub fn send_error<E: Into<Error>>(self, err: E) {
        // TODO: Should the result be dropped?
        let _ = self.tx.send(Err(err.into()));
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
            err_with: None,
        }
    }
}
