//! `Stream<Item = Request>` + `Service<Request>` => `Stream<Item = Response>`.

use super::{common, Error};
use futures_core::Stream;
use futures_util::stream::FuturesOrdered;
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// This is a `futures::Stream` of responses resulting from calling the wrapped `tower::Service`
/// for each request received on the wrapped `Stream`.
///
/// ```rust
/// # use std::task::{Poll, Context};
/// # use std::cell::Cell;
/// # use std::error::Error;
/// # use std::rc::Rc;
/// #
/// use futures_util::future::{ready, Ready};
/// use futures_util::StreamExt;
/// use tower_service::Service;
/// use tower_util::ServiceExt;
/// use tokio::prelude::*;
///
/// // First, we need to have a Service to process our requests.
/// #[derive(Debug, Eq, PartialEq)]
/// struct FirstLetter;
/// impl Service<&'static str> for FirstLetter {
///      type Response = &'static str;
///      type Error = Box<dyn Error + Send + Sync>;
///      type Future = Ready<Result<Self::Response, Self::Error>>;
///
///      fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
///          Poll::Ready(Ok(()))
///      }
///
///      fn call(&mut self, req: &'static str) -> Self::Future {
///          ready(Ok(&req[..1]))
///      }
/// }
///
/// #[tokio::main]
/// async fn main() {
///     // Next, we need a Stream of requests.
///     let (mut reqs, rx) = tokio::sync::mpsc::unbounded_channel();
///     // Note that we have to help Rust out here by telling it what error type to use.
///     // Specifically, it has to be From<Service::Error> + From<Stream::Error>.
///     let mut rsps = FirstLetter.call_all(rx);
///
///     // Now, let's send a few requests and then check that we get the corresponding responses.
///     reqs.send("one");
///     reqs.send("two");
///     reqs.send("three");
///     drop(reqs);
///
///     // We then loop over the response Strem that we get back from call_all.
///     let mut i = 0usize;
///     while let Some(rsp) = rsps.next().await {
///         // Each response is a Result (we could also have used TryStream::try_next)
///         match (i + 1, rsp.unwrap()) {
///             (1, "o") |
///             (2, "t") |
///             (3, "t") => {}
///             (n, i) => {
///                 unreachable!("{}. response was '{}'", n, i);
///             }
///         }
///         i += 1;
///     }
///
///     // And at the end, we can get the Service back when there are no more requests.
///     assert_eq!(rsps.into_inner(), FirstLetter);
/// }
/// ```
#[pin_project]
#[derive(Debug)]
pub struct CallAll<Svc, S>
where
    Svc: Service<S::Item>,
    S: Stream,
{
    #[pin]
    inner: common::CallAll<Svc, S, FuturesOrdered<Svc::Future>>,
}

impl<Svc, S> CallAll<Svc, S>
where
    Svc: Service<S::Item>,
    Svc::Error: Into<Error>,
    S: Stream,
{
    /// Create new `CallAll` combinator.
    ///
    /// Each request yielded by `stread` is passed to `svc`, and the resulting responses are
    /// yielded in the same order by the implementation of `Stream` for `CallAll`.
    pub fn new(service: Svc, stream: S) -> CallAll<Svc, S> {
        CallAll {
            inner: common::CallAll::new(service, stream, FuturesOrdered::new()),
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

    /// Return responses as they are ready, regardless of the initial order.
    ///
    /// This function must be called before the stream is polled.
    ///
    /// # Panics
    ///
    /// Panics if `poll` was called.
    pub fn unordered(self) -> super::CallAllUnordered<Svc, S> {
        self.inner.unordered()
    }
}

impl<Svc, S> Stream for CallAll<Svc, S>
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

impl<F: Future> common::Drive<F> for FuturesOrdered<F> {
    fn is_empty(&self) -> bool {
        FuturesOrdered::is_empty(self)
    }

    fn push(&mut self, future: F) {
        FuturesOrdered::push(self, future)
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Option<F::Output>> {
        Stream::poll_next(Pin::new(self), cx)
    }
}
