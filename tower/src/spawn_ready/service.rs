use super::future::ResponseFuture;
use crate::{util::ServiceExt, BoxError};
use futures_core::ready;
use futures_util::future::TryFutureExt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;
use tracing::Instrument;

/// Spawns tasks to drive an inner service to readiness.
///
/// See crate level documentation for more details.
#[derive(Debug)]
pub struct SpawnReady<S> {
    inner: Inner<S>,
}

#[derive(Debug)]
enum Inner<S> {
    Service(Option<S>),
    Future(tokio::task::JoinHandle<Result<S, BoxError>>),
}

impl<S> SpawnReady<S> {
    /// Creates a new [`SpawnReady`] wrapping `service`.
    pub fn new(service: S) -> Self {
        Self {
            inner: Inner::Service(Some(service)),
        }
    }
}

impl<S> Drop for SpawnReady<S> {
    fn drop(&mut self) {
        if let Inner::Future(ref mut task) = self.inner {
            task.abort();
        }
    }
}

impl<S, Req> Service<Req> for SpawnReady<S>
where
    Req: 'static,
    S: Service<Req> + Send + 'static,
    S::Error: Into<BoxError>,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future = ResponseFuture<S::Future, S::Error>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), BoxError>> {
        loop {
            self.inner = match self.inner {
                Inner::Service(ref mut svc) => {
                    if let Poll::Ready(r) = svc.as_mut().expect("illegal state").poll_ready(cx) {
                        return Poll::Ready(r.map_err(Into::into));
                    }

                    let svc = svc.take().expect("illegal state");
                    let rx =
                        tokio::spawn(svc.ready_oneshot().map_err(Into::into).in_current_span());
                    Inner::Future(rx)
                }
                Inner::Future(ref mut fut) => {
                    let svc = ready!(Pin::new(fut).poll(cx))??;
                    Inner::Service(Some(svc))
                }
            }
        }
    }

    fn call(&mut self, request: Req) -> Self::Future {
        match self.inner {
            Inner::Service(Some(ref mut svc)) => {
                ResponseFuture(svc.call(request).map_err(Into::into))
            }
            _ => unreachable!("poll_ready must be called"),
        }
    }
}

#[tokio::test]
async fn abort_on_drop() {
    use crate::util::ServiceExt;
    let (mock, mut handle) = tower_test::mock::pair::<(), ()>();
    let mut svc = SpawnReady::new(mock);
    handle.allow(0);

    // Drive the service to readiness until we signal a drop.
    let (drop_tx, drop_rx) = tokio::sync::oneshot::channel();
    let mut task = tokio_test::task::spawn(async move {
        tokio::select! {
            _ = drop_rx => {}
            _ = svc.ready() => unreachable!("Service must not become ready"),
        }
    });
    tokio_test::assert_pending!(task.poll());
    tokio_test::assert_pending!(handle.poll_request());

    // End the task and ensure that the inner service has been dropped.
    assert!(drop_tx.send(()).is_ok());
    tokio_test::assert_ready!(task.poll());
    assert!(tokio_test::assert_ready!(handle.poll_request()).is_none());
}
