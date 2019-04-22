use crate::{
    error::{Closed, Error, ServiceError},
    message::Message,
};
use futures::{try_ready, Async, Future, Poll, Stream};
use std::sync::{Arc, Mutex};
use tokio_executor::TypedExecutor;
use tokio_sync::mpsc;
use tower_service::Service;

/// Task that handles processing the buffer. This type should not be used
/// directly, instead `Buffer` requires an `Executor` that can accept this task.
///
/// The struct is `pub` in the private module and the type is *not* re-exported
/// as part of the public API. This is the "sealed" pattern to include "private"
/// types in public traits that are not meant for consumers of the library to
/// implement (only call).
pub struct Worker<T, Request>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    current_message: Option<Message<Request, T::Future>>,
    rx: mpsc::Receiver<Message<Request, T::Future>>,
    service: T,
    finish: bool,
    failed: Option<ServiceError>,
    handle: Handle,
}

/// Get the error out
pub(crate) struct Handle {
    inner: Arc<Mutex<Option<ServiceError>>>,
}

/// This trait allows you to use either Tokio's threaded runtime's executor or the `current_thread`
/// runtime's executor depending on if `T` is `Send` or `!Send`.
pub trait WorkerExecutor<T, Request>: TypedExecutor<Worker<T, Request>>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
}

impl<T, Request, E: TypedExecutor<Worker<T, Request>>> WorkerExecutor<T, Request> for E
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
}

impl<T, Request> Worker<T, Request>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    pub(crate) fn spawn<E>(
        service: T,
        rx: mpsc::Receiver<Message<Request, T::Future>>,
        executor: &mut E,
    ) -> Option<Handle>
    where
        E: WorkerExecutor<T, Request>,
    {
        let handle = Handle {
            inner: Arc::new(Mutex::new(None)),
        };

        let worker = Worker {
            current_message: None,
            finish: false,
            failed: None,
            rx,
            service,
            handle: handle.clone(),
        };

        match executor.spawn(worker) {
            Ok(()) => Some(handle),
            Err(_) => None,
        }
    }

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

    fn failed(&mut self, error: T::Error) {
        // The underlying service failed when we called `poll_ready` on it with the given `error`. We
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
        let error = ServiceError::new(error.into());

        let mut inner = self.handle.inner.lock().unwrap();

        if inner.is_some() {
            // Future::poll was called after we've already errored out!
            return;
        }

        *inner = Some(error.clone());
        drop(inner);

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
    T::Error: Into<Error>,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        if self.finish {
            return Ok(().into());
        }

        loop {
            match try_ready!(self.poll_next_msg()) {
                Some(msg) => {
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
                        }
                        Ok(Async::NotReady) => {
                            // Put out current message back in its slot.
                            self.current_message = Some(msg);
                            return Ok(Async::NotReady);
                        }
                        Err(e) => {
                            self.failed(e);
                            let _ = msg.tx.send(Err(self
                                .failed
                                .as_ref()
                                .expect("Worker::failed did not set self.failed?")
                                .clone()));
                        }
                    }
                }
                None => {
                    // No more more requests _ever_.
                    self.finish = true;
                    return Ok(Async::Ready(()));
                }
            }
        }
    }
}

impl Handle {
    pub(crate) fn get_error_on_closed(&self) -> Error {
        self.inner
            .lock()
            .unwrap()
            .as_ref()
            .map(|svc_err| svc_err.clone().into())
            .unwrap_or_else(|| Closed::new().into())
    }
}

impl Clone for Handle {
    fn clone(&self) -> Handle {
        Handle {
            inner: self.inner.clone(),
        }
    }
}
