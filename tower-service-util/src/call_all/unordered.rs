//! `Stream<Item = Request>` + `Service<Request>` => `Stream<Item = Response>`.

use super::{common, Error};
use futures::stream::FuturesUnordered;
use futures::{Future, Poll, Stream};
use tower_service::Service;

/// TODO: Dox
#[derive(Debug)]
pub struct CallAllUnordered<Svc, S>
where
    Svc: Service<S::Item>,
    S: Stream,
{
    inner: common::CallAll<Svc, S, FuturesUnordered<Svc::Future>>,
}

impl<Svc, S> CallAllUnordered<Svc, S>
where
    Svc: Service<S::Item>,
    Svc::Error: Into<Error>,
    S: Stream,
    S::Error: Into<Error>,
{
    /// Create new `CallAllUnordered` combinator.
    ///
    /// Each request yielded by `stread` is passed to `svc`, and the resulting responses are
    /// yielded in the same order by the implementation of `Stream` for
    /// `CallAllUnordered`.
    pub fn new(service: Svc, stream: S) -> CallAllUnordered<Svc, S> {
        CallAllUnordered {
            inner: common::CallAll::new(service, stream, FuturesUnordered::new()),
        }
    }

    /// Extract the wrapped `Service`.
    pub fn into_inner(self) -> Svc {
        self.inner.into_inner()
    }
}

impl<Svc, S> Stream for CallAllUnordered<Svc, S>
where
    Svc: Service<S::Item>,
    Svc::Error: Into<Error>,
    S: Stream,
    S::Error: Into<Error>,
{
    type Item = Svc::Response;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.inner.poll()
    }
}

impl<T: Future> common::Queue<T> for FuturesUnordered<T> {
    fn is_empty(&self) -> bool {
        FuturesUnordered::is_empty(self)
    }

    fn push(&mut self, future: T) {
        FuturesUnordered::push(self, future)
    }

    fn poll(&mut self) -> Poll<Option<T::Item>, T::Error> {
        Stream::poll(self)
    }
}
