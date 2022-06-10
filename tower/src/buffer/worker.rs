use super::{
    error::{Closed, ServiceError},
    message::Message,
};
use futures_core::ready;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;
use tower_service::{Call, Service};

pin_project_lite::pin_project! {

    /// Task that handles processing the buffer. This type should not be used
    /// directly, instead `Buffer` requires an `Executor` that can accept this task.
    ///
    /// The struct is `pub` in the private module and the type is *not* re-exported
    /// as part of the public API. This is the "sealed" pattern to include "private"
    /// types in public traits that are not meant for consumers of the library to
    /// implement (only call).

    #[derive(Debug)]
    pub struct Worker<T, Request, F> {
        current_message: Option<Message<Request, F>>,
        rx: mpsc::Receiver<Message<Request, F>>,
        service: T,
        finish: bool,
        failed: Option<ServiceError>,
        handle: Handle,
    }
}

/// Get the error out
#[derive(Debug)]
pub(crate) struct Handle {
    inner: Arc<HandleInner>,
}

#[derive(Debug)]
struct HandleInner {
    error: Mutex<Option<ServiceError>>,
    closed: AtomicBool,
}

impl<T, Request, F, Error> Worker<T, Request, F>
where
    T: for<'a> Service<'a, Request, Future = F, Error = Error>,
    Error: Into<crate::BoxError>,
{
    pub(crate) fn new(
        service: T,
        rx: mpsc::Receiver<Message<Request, F>>,
    ) -> (Handle, Worker<T, Request, F>) {
        let handle = Handle {
            inner: Arc::new(HandleInner {
                closed: AtomicBool::new(false),
                error: Mutex::new(None),
            }),
        };

        let worker = Worker {
            current_message: None,
            finish: false,
            failed: None,
            rx,
            service,
            handle: handle.clone(),
        };

        (handle, worker)
    }

    /// Return the next queued Message that hasn't been canceled.
    ///
    /// If a `Message` is returned, the `bool` is true if this is the first time we received this
    /// message, and false otherwise (i.e., we tried to forward it to the backing service before).
    fn poll_next_msg(&mut self, cx: &mut Context<'_>) -> Poll<Option<(Message<Request, F>, bool)>> {
        if self.finish {
            // We've already received None and are shutting down
            return Poll::Ready(None);
        }

        tracing::trace!("worker polling for next message");
        if let Some(msg) = self.current_message.take() {
            // If the oneshot sender is closed, then the receiver is dropped,
            // and nobody cares about the response. If this is the case, we
            // should continue to the next request.
            if !msg.tx.is_closed() {
                tracing::trace!("resuming buffered request");
                return Poll::Ready(Some((msg, false)));
            }

            tracing::trace!("dropping cancelled buffered request");
        }

        // Get the next request
        while let Some(msg) = ready!(Pin::new(&mut self.rx).poll_recv(cx)) {
            if !msg.tx.is_closed() {
                tracing::trace!("processing new request");
                return Poll::Ready(Some((msg, true)));
            }
            // Otherwise, request is canceled, so pop the next one.
            tracing::trace!("dropping cancelled request");
        }

        Poll::Ready(None)
    }
}

impl<T, Request, F, Error> Future for Worker<T, Request, F>
where
    T: for<'a> Service<'a, Request, Future = F, Error = Error>,
    Error: Into<crate::BoxError>,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.finish {
            return Poll::Ready(());
        }
        let this = self.get_mut();

        loop {
            match ready!(this.poll_next_msg(cx)) {
                Some((msg, first)) => {
                    let _guard = msg.span.enter();
                    if let Some(ref failed) = this.failed {
                        tracing::trace!("notifying caller about worker failure");
                        let _ = msg.tx.send(Err(failed.clone()));
                        continue;
                    }

                    // Wait for the service to be ready
                    tracing::trace!(
                        resumed = !first,
                        message = "worker received request; waiting for service readiness"
                    );
                    let Self {
                        ref mut current_message,
                        ref mut rx,
                        ref mut service,
                        ref mut failed,
                        ref handle,
                        ..
                    } = this;
                    match service.poll_ready(cx) {
                        Poll::Ready(Ok(call)) => {
                            tracing::debug!(service.ready = true, message = "processing request");
                            let response = call.call(msg.request);

                            // Send the response future back to the sender.
                            //
                            // An error means the request had been canceled in-between
                            // our calls, the response future will just be dropped.
                            tracing::trace!("returning response future");
                            let _ = msg.tx.send(Ok(response));
                        }
                        Poll::Pending => {
                            tracing::trace!(service.ready = false, message = "delay");
                            // Put out current message back in its slot.
                            drop(_guard);
                            *current_message = Some(msg);
                            return Poll::Pending;
                        }
                        Poll::Ready(Err(e)) => {
                            let error = e.into();
                            tracing::debug!({ %error }, "service failed");
                            drop(_guard);
                            handle.inner.closed.store(true, Ordering::Release);
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
                            let error = ServiceError::new(error);

                            let mut inner = handle.inner.error.lock().unwrap();

                            if inner.is_none() {
                                *inner = Some(error.clone());
                                drop(inner);

                                rx.close();

                                // By closing the mpsc::Receiver, we know that poll_next_msg will soon return Ready(None),
                                // which will trigger the `self.finish == true` phase. We just need to make sure that any
                                // requests that we receive before we've exhausted the receiver receive the error:
                                *failed = Some(error);
                                let _ = msg.tx.send(Err(failed
                                    .as_ref()
                                    .expect("Worker::failed did not set self.failed?")
                                    .clone()));
                            }
                        }
                    }
                }
                None => {
                    // No more more requests _ever_.
                    this.finish = true;
                    return Poll::Ready(());
                }
            }
        }
    }
}

impl Handle {
    #[inline(always)]
    pub(crate) fn try_get_error(&self) -> Option<crate::BoxError> {
        if !self.inner.closed.load(Ordering::Acquire) {
            return None;
        }

        Some(self.get_error_on_closed())
    }

    #[inline(never)]
    #[cold]
    pub(crate) fn get_error_on_closed(&self) -> crate::BoxError {
        self.inner
            .error
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
