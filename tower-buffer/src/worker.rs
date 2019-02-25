use error::ServiceError;
use futures::future::Executor;
use futures::{Async, Future, Poll, Stream};
use message::Message;
use std::sync::{Arc, Mutex};
use tokio_sync::mpsc;
use tower_service::Service;

/// Task that handles processing the buffer. This type should not be used
/// directly, instead `Buffer` requires an `Executor` that can accept this task.
pub struct Worker<T, Request>
where
    T: Service<Request>,
{
    current_message: Option<Message<Request, T::Future, T::Error>>,
    rx: mpsc::Receiver<Message<Request, T::Future, T::Error>>,
    service: T,
    finish: bool,
    failed: Option<Arc<ServiceError<T::Error>>>,
    handle: Handle<T::Error>,
}

/// Get the error out
pub(crate) struct Handle<E> {
    inner: Arc<Mutex<Option<Arc<ServiceError<E>>>>>,
}

/// This trait allows you to use either Tokio's threaded runtime's executor or the `current_thread`
/// runtime's executor depending on if `T` is `Send` or `!Send`.
pub trait WorkerExecutor<T, Request>: Executor<Worker<T, Request>>
where
    T: Service<Request>,
{
}

impl<T, Request, E: Executor<Worker<T, Request>>> WorkerExecutor<T, Request> for E where
    T: Service<Request>
{
}

impl<T, Request> Worker<T, Request>
where
    T: Service<Request>,
{
    pub(crate) fn spawn<E>(
        service: T,
        rx: mpsc::Receiver<Message<Request, T::Future, T::Error>>,
        executor: &E,
    ) -> Result<Handle<T::Error>, T>
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

        match executor.execute(worker) {
            Ok(()) => Ok(handle),
            Err(err) => Err(err.into_future().service),
        }
    }

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
        let error = Arc::new(ServiceError::new(method, error));

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

impl<E> Handle<E> {
    pub(crate) fn get_error_on_closed(&self) -> Arc<ServiceError<E>> {
        self.inner
            .lock()
            .unwrap()
            .as_ref()
            .expect("Worker exited, but did not set error.")
            .clone()
    }
}

impl<E> Clone for Handle<E> {
    fn clone(&self) -> Handle<E> {
        Handle {
            inner: self.inner.clone(),
        }
    }
}
