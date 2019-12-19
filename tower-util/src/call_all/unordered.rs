//! `Stream<Item = Request>` + `Service<Request>` => `Stream<Item = Response>`.

use super::{common, Error};
use futures_core::Stream;
use futures_util::stream::FuturesUnordered;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// A stream of responses received from the inner service in received order.
///
/// Similar to `CallAll` except, instead of yielding responses in request order,
/// responses are returned as they are available.
#[pin_project]
#[derive(Debug)]
pub struct CallAllUnordered<Svc, S>
where
    Svc: Service<S::Item>,
    S: Stream,
{
    #[pin]
    inner: common::CallAll<Svc, S, FuturesUnordered<Svc::Future>>,
}

impl<Svc, S> CallAllUnordered<Svc, S>
where
    Svc: Service<S::Item>,
    Svc::Error: Into<Error>,
    S: Stream,
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
    ///
    /// # Panics
    ///
    /// Panics if `take_service` was already called.
    pub fn into_inner(self) -> Svc {
        self.inner.into_inner()
    }

    /// Extract the wrapped `Service`.
    ///
    /// This `CallAll` can no longer be used after this function has been called.
    ///
    /// # Panics
    ///
    /// Panics if `take_service` was already called.
    pub fn take_service(self: Pin<&mut Self>) -> Svc {
        self.project().inner.take_service()
    }
}

impl<Svc, S> Stream for CallAllUnordered<Svc, S>
where
    Svc: Service<S::Item>,
    Svc::Error: Into<Error>,
    S: Stream,
{
    type Item = Result<Svc::Response, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

impl<F: Future> common::Drive<F> for FuturesUnordered<F> {
    fn is_empty(&self) -> bool {
        FuturesUnordered::is_empty(self)
    }

    fn push(&mut self, future: F) {
        FuturesUnordered::push(self, future)
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Option<F::Output>> {
        Stream::poll_next(Pin::new(self), cx)
    }
}
