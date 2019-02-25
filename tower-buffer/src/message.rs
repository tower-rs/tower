use error::ServiceError;
use tokio_sync::oneshot;
use std::sync::Arc;

/// Message sent over buffer
#[derive(Debug)]
pub(crate) struct Message<Request, Fut, E> {
    pub(crate) request: Request,
    pub(crate) tx: Tx<Fut, E>,
}

/// Response sender
pub(crate) type Tx<Fut, E> = oneshot::Sender<Result<Fut, Arc<ServiceError<E>>>>;

/// Response receiver
pub(crate) type Rx<Fut, E> = oneshot::Receiver<Result<Fut, Arc<ServiceError<E>>>>;
