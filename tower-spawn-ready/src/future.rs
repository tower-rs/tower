//! Background readiness types

use crate::Error;
use futures_core::ready;
use pin_project::pin_project;
use std::marker::PhantomData;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use tower_service::Service;

/// Drives a service to readiness.
#[pin_project]
#[derive(Debug)]
pub struct BackgroundReady<T, Request> {
    service: Option<T>,
    tx: Option<oneshot::Sender<Result<T, Error>>>,
    _req: PhantomData<Request>,
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
