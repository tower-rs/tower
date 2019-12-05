//! Mock `Service` that can be used in tests.

pub mod error;
pub mod future;
pub mod spawn;

pub use spawn::Spawn;

use crate::mock::{error::Error, future::ResponseFuture};
use core::task::Waker;

use tokio::sync::{mpsc, oneshot};
use tower_layer::Layer;
use tower_service::Service;

use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    u64,
};

/// Spawn a layer onto a mock service.
pub fn spawn_layer<T, U, L>(layer: L) -> (Spawn<L::Service>, Handle<T, U>)
where
    L: Layer<Mock<T, U>>,
{
    let (inner, handle) = pair();
    let svc = layer.layer(inner);

    (Spawn::new(svc), handle)
}

/// Spawn a Service onto a mock task.
pub fn spawn<T, U>() -> (Spawn<Mock<T, U>>, Handle<T, U>) {
    let (svc, handle) = pair();

    (Spawn::new(svc), handle)
}

/// Spawn a Service via the provided wrapper closure.
pub fn spawn_with<T, U, F, S>(f: F) -> (Spawn<S>, Handle<T, U>)
where
    F: Fn(Mock<T, U>) -> S,
{
    let (svc, handle) = pair();

    let svc = f(svc);

    (Spawn::new(svc), handle)
}

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
    tasks: HashMap<u64, Waker>,

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

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut state = self.state.lock().unwrap();

        if state.is_closed {
            return Poll::Ready(Err(error::Closed::new().into()));
        }

        if let Some(e) = state.err_with.take() {
            return Poll::Ready(Err(e));
        }

        if self.can_send {
            return Poll::Ready(Ok(()));
        }

        if state.rem > 0 {
            assert!(!state.tasks.contains_key(&self.id));

            // Returning `Ready` means the next call to `call` must succeed.
            self.can_send = true;

            Poll::Ready(Ok(()))
        } else {
            // Bit weird... but whatevz
            *state
                .tasks
                .entry(self.id)
                .or_insert_with(|| cx.waker().clone()) = cx.waker().clone();

            Poll::Pending
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

        match self.tx.lock().unwrap().send((request, send_response)) {
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
    pub fn poll_request(&mut self) -> Poll<Option<Request<T, U>>> {
        tokio_test::task::spawn(()).enter(|cx, _| Box::pin(self.rx.recv()).as_mut().poll(cx))
    }

    /// Gets the next request.
    pub async fn next_request(&mut self) -> Option<Request<T, U>> {
        self.rx.recv().await
    }

    /// Allow a certain number of requests
    pub fn allow(&mut self, num: u64) {
        let mut state = self.state.lock().unwrap();
        state.rem = num;

        if num > 0 {
            for (_, task) in state.tasks.drain() {
                task.wake();
            }
        }
    }

    /// Make the next poll_ method error with the given error.
    pub fn send_error<E: Into<Error>>(&mut self, e: E) {
        let mut state = self.state.lock().unwrap();
        state.err_with = Some(e.into());

        for (_, task) in state.tasks.drain() {
            task.wake();
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
            task.wake();
        }
    }
}

// ===== impl SendResponse =====

impl<T> SendResponse<T> {
    /// Resolve the pending request future for the linked request with the given response.
    pub fn send_response(self, response: T) {
        // TODO: Should the result be dropped?
        let _ = self.tx.send(Ok(response));
    }

    /// Resolve the pending request future for the linked request with the given error.
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
