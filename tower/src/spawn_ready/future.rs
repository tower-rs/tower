//! Background readiness types

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
    tx: Option<oneshot::Sender<Result<T, crate::BoxError>>>,
    _req: PhantomData<Request>,
}

opaque_future! {
    /// Response future from [`SpawnReady`] services.
    ///
    /// [`SpawnReady`]: crate::spawn_ready::SpawnReady
    pub type ResponseFuture<F, E> = futures_util::future::MapErr<F, fn(E) -> crate::BoxError>;


pub(crate) fn background_ready<T, Request>(
    service: T,
) -> (
    BackgroundReady<T, Request>,
    oneshot::Receiver<Result<T, crate::BoxError>>,
)
where
    T: Service<Request>,
    T::Error: Into<crate::BoxError>,
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
    T::Error: Into<crate::BoxError>,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // Is the channel sender closed?
        // Note that we must actually poll the sender's closed future here,
        // rather than just calling `is_closed` on it, since we want to be
        // notified if the receiver is dropped.
        if let Poll::Ready(_) = this.tx.as_mut().expect("illegal state").poll_closed(cx) {
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
