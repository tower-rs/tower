use crate::error::Error;
use futures::{Future, Poll};

pub struct ResponseFuture<F>(F);

impl<F> ResponseFuture<F> {
    pub(crate) fn new(future: F) -> ResponseFuture<F> {
        ResponseFuture(future)
    }
}

impl<F> Future for ResponseFuture<F>
where
    F: Future,
    F::Error: Into<Error>,
{
    type Item = F::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll().map_err(Into::into)
    }
}
