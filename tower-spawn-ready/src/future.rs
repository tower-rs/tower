//! Background readiness types

use crate::error::Error;
use futures_core::ready;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_executor::TypedExecutor;
use tokio_sync::oneshot;
use tower_service::Service;
use tower_util::Ready;

#[pin_project]
/// Drives a service to readiness.
pub struct BackgroundReady<T, Request>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    #[pin]
    ready: Ready<T, Request>,
    tx: Option<oneshot::Sender<Result<T, Error>>>,
}

/// This trait allows you to use either Tokio's threaded runtime's executor or
/// the `current_thread` runtime's executor depending on if `T` is `Send` or
/// `!Send`.
pub trait BackgroundReadyExecutor<T, Request>: TypedExecutor<BackgroundReady<T, Request>>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
}

impl<T, Request, E> BackgroundReadyExecutor<T, Request> for E
where
    E: TypedExecutor<BackgroundReady<T, Request>>,
    T: Service<Request>,
    T::Error: Into<Error>,
{
}

pub(crate) fn background_ready<T, Request>(
    service: T,
) -> (
    BackgroundReady<T, Request>,
    oneshot::Receiver<Result<T, Error>>,
)
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    let (tx, rx) = oneshot::channel();
    let bg = BackgroundReady {
        ready: Ready::new(service),
        tx: Some(tx),
    };
    (bg, rx)
}

impl<T, Request> Future for BackgroundReady<T, Request>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if let Poll::Ready(_) = Pin::new(this.tx.as_mut().expect("illegal state")).poll_closed(cx) {
            return Poll::Ready(());
        }

        let result = ready!(this.ready.poll(cx));
        let _ = this
            .tx
            .take()
            .expect("illegal state")
            .send(result.map_err(Into::into));

        Poll::Ready(())
    }
}
