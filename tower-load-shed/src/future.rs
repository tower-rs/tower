use std::fmt;

use futures::{Future, Poll};

use error::{Error, Overloaded};

/// Future for the `LoadShed` service.
pub struct ResponseFuture<F> {
    state: Result<F, ()>,
}

impl<F> ResponseFuture<F> {
    pub(crate) fn called(fut: F) -> Self {
        ResponseFuture { state: Ok(fut) }
    }

    pub(crate) fn overloaded() -> Self {
        ResponseFuture { state: Err(()) }
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
        match self.state {
            Ok(ref mut fut) => fut.poll().map_err(Into::into),
            Err(()) => Err(Overloaded::new().into()),
        }
    }
}

impl<F> fmt::Debug for ResponseFuture<F>
where
    // bounds for future-proofing...
    F: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("ResponseFuture")
    }
}
