use crate::{Change, Discover};
use futures_core::{ready, TryStream};
use pin_project::pin_project;
use std::hash::Hash;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// Dynamic service discovery based on a stream of service changes.
#[pin_project]
#[derive(Debug)]
pub struct ServiceStream<S> {
    #[pin]
    inner: S,
}

impl<S> ServiceStream<S> {
    #[allow(missing_docs)]
    pub fn new<K, Svc, Request>(services: S) -> Self
    where
        S: TryStream<Ok = Change<K, Svc>>,
        K: Hash + Eq,
        Svc: Service<Request>,
    {
        ServiceStream { inner: services }
    }
}

impl<S, K, Svc> Discover for ServiceStream<S>
where
    K: Hash + Eq,
    S: TryStream<Ok = Change<K, Svc>>,
{
    type Key = K;
    type Service = Svc;
    type Error = S::Error;

    fn poll_discover(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Change<Self::Key, Self::Service>, Self::Error>> {
        match ready!(self.project().inner.try_poll_next(cx)).transpose()? {
            Some(c) => Poll::Ready(Ok(c)),
            None => {
                // there are no more service changes coming
                Poll::Pending
            }
        }
    }
}
