use tokio::sync::oneshot;

/// Message sent over buffer
#[derive(Debug)]
pub(crate) struct Message<Request, Fut, E> {
    pub(crate) request: Request,
    pub(crate) tx: Tx<Fut, E>,
    pub(crate) span: tracing::Span,
}

/// Response sender
pub(crate) type Tx<Fut, E> = oneshot::Sender<Result<Fut, E>>;

/// Response receiver
pub(crate) type Rx<Fut, E> = oneshot::Receiver<Result<Fut, E>>;
