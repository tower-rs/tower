use crate::error::Error;
use futures::{Async, Future, Poll};
use tokio_executor::TypedExecutor;
use tokio_sync::oneshot;
use tower_service::Service;
use tower_util::Ready;

pub struct BackgroundReady<T, Request>
where
    T: Service<Request>,
    T::Error: Into<Error>,
{
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
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        match self.tx.as_mut().expect("illegal state").poll_close() {
            Ok(Async::Ready(())) | Err(()) => return Err(()),
            Ok(Async::NotReady) => {}
        }

        let result = match self.ready.poll() {
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(svc)) => Ok(svc),
            Err(e) => Err(e.into()),
        };

        self.tx
            .take()
            .expect("illegal state")
            .send(result)
            .map_err(|_| ())?;

        Ok(Async::Ready(()))
    }
}
