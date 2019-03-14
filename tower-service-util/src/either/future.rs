use super::Error;
use _either::Either;
use futures::{Future, Poll};

#[derive(Debug)]
pub struct ResponseFuture<A, B> {
    inner: Either<A, B>,
}

impl<A, B> ResponseFuture<A, B> {
    pub(crate) fn new_left(left: A) -> ResponseFuture<A, B> {
        ResponseFuture { inner: Either::Left(left) }
    }

    pub(crate) fn new_right(right: B) -> ResponseFuture<A, B> {
        ResponseFuture { inner: Either::Right(right) }
    }
}

impl<A, B> Future for ResponseFuture<A, B>
where
    A: Future,
    A::Error: Into<Error>,
    B: Future<Item = A::Item>,
    B::Error: Into<Error>,
{
    type Item = A::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::Either::*;

        match &mut self.inner {
            Left(fut) => fut.poll().map_err(Into::into),
            Right(fut) => fut.poll().map_err(Into::into),
        }
    }
}
