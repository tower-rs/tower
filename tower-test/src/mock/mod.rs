//! Mock [`Service`]s for use in tests.
//!
//! See the [crate-level documentation](crate) for an overview and an example.
//!
//! [`Service`]: tower_service::Service

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
};

/// Apply a [`Layer`] to a mock [`Service`] and spawn the result on a mock task.
///
/// Returns the layered service wrapped in a [`Spawn`], along with the [`Handle`]
/// for the underlying [`Mock`].
pub fn spawn_layer<T, U, L>(layer: L) -> (Spawn<L::Service>, Handle<T, U>)
where
    L: Layer<Mock<T, U>>,
{
    let (inner, handle) = pair();
    let svc = layer.layer(inner);

    (Spawn::new(svc), handle)
}

/// Create a mock [`Service`] spawned on a mock task.
///
/// The returned [`Spawn`] wraps a [`Mock`] so that its readiness can be polled
/// synchronously in tests; the paired [`Handle`] is used to receive requests
/// and send responses. See [`pair`] for the un-spawned equivalent.
pub fn spawn<T, U>() -> (Spawn<Mock<T, U>>, Handle<T, U>) {
    let (svc, handle) = pair();

    (Spawn::new(svc), handle)
}

/// Create a mock [`Service`], pass it through `f`, and spawn the result on a
/// mock task.
///
/// This is like [`spawn()`], but the closure `f` may wrap the [`Mock`] in
/// additional middleware before it is spawned.
pub fn spawn_with<T, U, F, S>(f: F) -> (Spawn<S>, Handle<T, U>)
where
    F: Fn(Mock<T, U>) -> S,
{
    let (svc, handle) = pair();

    let svc = f(svc);

    (Spawn::new(svc), handle)
}

/// A mock [`Service`].
///
/// Every request is forwarded to the paired [`Handle`], which decides whether
/// and how to respond. Construct one with [`pair`] (or one of the `spawn*`
/// functions). Cloning a `Mock` produces another service backed by the same
/// [`Handle`], so a single handle can observe the requests of every clone.
#[derive(Debug)]
pub struct Mock<T, U> {
    id: u64,
    tx: Mutex<Tx<T, U>>,
    state: Arc<Mutex<State>>,
    can_send: bool,
}

/// Drives a paired [`Mock`].
///
/// A `Handle` receives the requests made to its [`Mock`] (via
/// [`next_request`]/[`poll_request`], each of which yields a [`SendResponse`]
/// for replying), can fail the mock's readiness with [`send_error`], and can
/// limit how many requests the mock accepts with [`allow`].
///
/// [`next_request`]: Handle::next_request
/// [`poll_request`]: Handle::poll_request
/// [`send_error`]: Handle::send_error
/// [`allow`]: Handle::allow
#[derive(Debug)]
pub struct Handle<T, U> {
    rx: Rx<T, U>,
    state: Arc<Mutex<State>>,
}

type Request<T, U> = (T, SendResponse<U>);

/// Sends a response (or error) back for a single request received by a [`Mock`].
///
/// Returned, paired with the request, by [`Handle::next_request`] and
/// [`Handle::poll_request`] (and by the [`assert_request_eq!`] macro).
///
/// [`assert_request_eq!`]: crate::assert_request_eq
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

/// Create a [`Mock`] [`Service`] paired with its [`Handle`].
///
/// By default the mock accepts any number of requests (its `poll_ready` is
/// always ready); use [`Handle::allow`] to apply backpressure.
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
    /// Polls for the next request made to the [`Mock`].
    ///
    /// On [`Ready`], yields the request together with a [`SendResponse`] used to
    /// reply to it, or [`None`] once every [`Mock`] clone has been dropped.
    ///
    /// [`Ready`]: std::task::Poll::Ready
    pub fn poll_request(&mut self) -> Poll<Option<Request<T, U>>> {
        tokio_test::task::spawn(()).enter(|cx, _| Box::pin(self.rx.recv()).as_mut().poll(cx))
    }

    /// Waits for the next request made to the [`Mock`].
    ///
    /// Resolves to the request together with a [`SendResponse`] used to reply to
    /// it, or [`None`] once every [`Mock`] clone has been dropped.
    pub async fn next_request(&mut self) -> Option<Request<T, U>> {
        self.rx.recv().await
    }

    /// Allow the [`Mock`] to accept `num` more requests.
    ///
    /// Once the mock has accepted that many requests, its `poll_ready` returns
    /// [`Pending`] until `allow` is called again. A newly-created mock starts
    /// out allowing `u64::MAX` requests, so this is only needed to exert
    /// backpressure in a test.
    ///
    /// [`Pending`]: std::task::Poll::Pending
    pub fn allow(&mut self, num: u64) {
        let mut state = self.state.lock().unwrap();
        state.rem = num;

        if num > 0 {
            for (_, task) in state.tasks.drain() {
                task.wake();
            }
        }
    }

    /// Make the [`Mock`]'s next `poll_ready` resolve to the given error.
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
    /// Resolve the request's response future with the given response.
    pub fn send_response(self, response: T) {
        // TODO: Should the result be dropped?
        let _ = self.tx.send(Ok(response));
    }

    /// Resolve the request's response future with the given error.
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
