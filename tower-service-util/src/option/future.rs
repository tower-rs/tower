use super::{error, Error};
use futures::{Future, Poll};

/// Response future returned by `OptionService`.
pub struct ResponseFuture<T> {
    inner: Option<T>,
}

impl<T> ResponseFuture<T> {
    pub(crate) fn new(inner: Option<T>) -> ResponseFuture<T> {
        ResponseFuture { inner }
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
        match self.inner {
            Some(ref mut inner) => inner.poll().map_err(Into::into),
            None => Err(error::None::new().into()),
        }
    }
}
