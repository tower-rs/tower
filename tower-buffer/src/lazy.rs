use crate::{error::Error, future::ResponseFuture, service::Buffer, worker::WorkerExecutor};
use futures::{Async, Poll};
use std::sync::{Arc, Mutex};
use tokio_executor::DefaultExecutor;
use tower_service::Service;

#[derive(Clone)]
pub struct BufferLazy<T, Request, E>
where
    T: Service<Request>,
{
    inner: Arc<Mutex<Option<Buffer<T, Request>>>>,
    state: State<T, Request>,
    executor: E,
}

#[derive(Clone)]
enum State<T, Request>
where
    T: Service<Request>,
{
    Waiting(Option<T>, usize),
    Spawned(Buffer<T, Request>),
}

impl<T, Request> BufferLazy<T, Request, DefaultExecutor>
where
    T: Service<Request>,
{
    pub fn new(svc: T, bound: usize) -> Self {
        BufferLazy {
            inner: Arc::new(Mutex::new(None)),
            state: State::Waiting(Some(svc), bound),
            executor: DefaultExecutor::current(),
        }
    }
}

impl<T, Request, E> BufferLazy<T, Request, E>
where
    T: Service<Request>,
    T::Error: Into<Error>,
    E: WorkerExecutor<T, Request> + Clone,
{
    pub fn with_executor(svc: T, bound: usize, executor: E) -> Self {
        BufferLazy {
            inner: Arc::new(Mutex::new(None)),
            state: State::Waiting(Some(svc), bound),
            executor,
        }
    }
}

impl<T, Request, E> Service<Request> for BufferLazy<T, Request, E>
where
    T: Service<Request> + Send + 'static,
    T::Future: Send,
    T::Response: Send,
    T::Error: Into<Error> + Send + Sync,
    Request: Send + 'static,
    E: WorkerExecutor<T, Request> + Clone,
{
    type Response = T::Response;
    type Error = Error;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        match self.state {
            State::Waiting(ref mut svc, bound) => {
                let mut inner = self.inner.lock().unwrap();
                let state = if inner.is_some() {
                    State::Spawned(inner.clone().unwrap())
                } else {
                    let svc = svc.take().unwrap();
                    let buffer =
                        // TODO: this should return the error on poll_ready
                        Buffer::with_executor(svc, bound, &mut self.executor.clone()).unwrap();
                    *inner = Some(buffer.clone());
                    State::Spawned(buffer)
                };
                drop(inner);

                self.state = state;
                return Ok(Async::Ready(()));
            }

            State::Spawned(ref mut buf) => return buf.poll_ready(),
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        match &mut self.state {
            State::Waiting(_, _) => {
                panic!("Did not call poll_ready!");
            }

            State::Spawned(buf) => buf.call(request),
        }
    }
}
