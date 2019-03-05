use Shared;
use futures::{Future, Poll};
use std::sync::Arc;

#[derive(Debug)]
pub struct ResponseFuture<T> {
    inner: T,
    shared: Arc<Shared>,
}

impl<T> ResponseFuture<T> {
    pub(crate) fn new(inner: T, shared: Arc<Shared>) -> ResponseFuture<T> {
        ResponseFuture { inner, shared }
    }
}

impl<T> Future for ResponseFuture<T>
where
    T: Future,
{
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll()
    }
}

impl<T> Drop for ResponseFuture<T> {
    fn drop(&mut self) {
        self.shared.release();
    }
}
