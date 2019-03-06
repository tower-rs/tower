use Error;
use futures::{Future, Poll};
use tokio_sync::semaphore::Semaphore;
use std::sync::Arc;

#[derive(Debug)]
pub struct ResponseFuture<T> {
    inner: T,
    semaphore: Arc<Semaphore>,
}

impl<T> ResponseFuture<T> {
    pub(crate) fn new(inner: T, semaphore: Arc<Semaphore>) -> ResponseFuture<T> {
        ResponseFuture { inner, semaphore }
    }
}

impl<T> Future for ResponseFuture<T>
where
    T: Future,
    T::Error: Into<Error>,
{
    type Item = T::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll()
            .map_err(Into::into)
    }
}

impl<T> Drop for ResponseFuture<T> {
    fn drop(&mut self) {
        self.semaphore.add_permits(1);
    }
}
