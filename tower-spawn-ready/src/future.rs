//! Background readiness types

use crate::error::Error;
use futures_core::ready;
use pin_project::pin_project;
use std::marker::PhantomData;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_executor::TypedExecutor;
use tokio_sync::oneshot;
use tower_service::Service;

/// Drives a service to readiness.
#[pin_project]
#[derive(Debug)]
pub struct BackgroundReady<T, Request> {
    service: Option<T>,
    tx: Option<oneshot::Sender<Result<T, Error>>>,
    _req: PhantomData<Request>,
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
        service: Some(service),
        tx: Some(tx),
        _req: PhantomData,
    };
    (bg, rx)
}

impl<T, Request> Future for BackgroundReady<T, Request>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if let Poll::Ready(_) = Pin::new(this.tx.as_mut().expect("illegal state")).poll_closed(cx) {
            return Poll::Ready(());
        }

        let result = ready!(this.service.as_mut().expect("illegal state").poll_ready(cx))
            .map(|()| this.service.take().expect("illegal state"));

        let _ = this
            .tx
            .take()
            .expect("illegal state")
            .send(result.map_err(Into::into));

        Poll::Ready(())
    }
}
