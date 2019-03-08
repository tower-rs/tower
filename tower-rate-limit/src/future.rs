use crate::Error;
use futures::{Poll, Future};

#[derive(Debug)]
pub struct ResponseFuture<T> {
    inner: T,
}

impl<T> ResponseFuture<T> {
    pub(crate) fn new(inner: T) -> ResponseFuture<T> {
        ResponseFuture { inner }
    }
}

impl<T> Future for ResponseFuture<T>
where
    T: Future,
    Error: From<T::Error>,
{
    type Item = T::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll().map_err(Into::into)
    }
}
