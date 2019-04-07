use crate::{Change, Discover};
use futures::{try_ready, Async, Poll, Stream};
use std::hash::Hash;
use tower_service::Service;

/// Dynamic service discovery based on a stream of service changes.
pub struct ServiceStream<S> {
    inner: futures::stream::Fuse<S>,
}

impl<S> ServiceStream<S> {
    pub fn new<K, Svc, Request>(services: S) -> Self
    where
        S: Stream<Item = Change<K, Svc>>,
        K: Hash + Eq,
        Svc: Service<Request>,
    {
        ServiceStream {
            inner: services.fuse(),
        }
    }
}

impl<S, K, Svc> Discover for ServiceStream<S>
where
    K: Hash + Eq,
    S: Stream<Item = Change<K, Svc>>,
{
    type Key = K;
    type Service = Svc;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::Error> {
        match try_ready!(self.inner.poll()) {
            Some(c) => Ok(Async::Ready(c)),
            None => {
                // there are no more service changes coming
                Ok(Async::NotReady)
            }
        }
    }
}
