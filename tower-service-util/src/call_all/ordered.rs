//! `Stream<Item = Request>` + `Service<Request>` => `Stream<Item = Response>`.

use super::{common, Error};
use futures::stream::FuturesOrdered;
use futures::{Future, Poll, Stream};
use tower_service::Service;

/// This is a `futures::Stream` of responses resulting from calling the wrapped `tower::Service`
/// for each request received on the wrapped `Stream`.
///
/// ```rust
/// # extern crate futures;
/// # extern crate tower_service;
/// # extern crate tokio_mock_task;
/// # extern crate tower;
/// # use futures::future::{ok, FutureResult};
/// # use futures::{Async, Poll};
/// # use std::cell::Cell;
/// # use std::rc::Rc;
/// #
/// use futures::Stream;
/// use tower_service::Service;
/// use tower::ServiceExt;
///
/// // First, we need to have a Service to process our requests.
/// #[derive(Debug, Eq, PartialEq)]
/// struct FirstLetter;
/// impl Service<&'static str> for FirstLetter {
///      type Response = &'static str;
///      type Error = ();
///      type Future = FutureResult<Self::Response, ()>;
///
///      fn poll_ready(&mut self) -> Poll<(), Self::Error> {
///          Ok(Async::Ready(()))
///      }
///
///      fn call(&mut self, req: &'static str) -> Self::Future {
///          ok(&req[..1])
///      }
/// }
///
/// # fn main() {
/// # let mut mock = tokio_mock_task::MockTask::new();
/// // Next, we need a Stream of requests.
/// let (reqs, rx) = futures::unsync::mpsc::unbounded();
/// // Note that we have to help Rust out here by telling it what error type to use.
/// // Specifically, it has to be From<Service::Error> + From<Stream::Error>.
/// let rsps = FirstLetter.call_all::<_, ()>(rx);
///
/// // Now, let's send a few requests and then check that we get the corresponding responses.
/// reqs.unbounded_send("one");
/// reqs.unbounded_send("two");
/// reqs.unbounded_send("three");
/// drop(reqs);
///
/// // We then loop over the response Strem that we get back from call_all.
/// # // a little bit of trickery here since we don't have an executor
/// # /*
/// let mut iter = rsps.wait();
/// # */
/// # let mut iter = mock.enter(|| rsps.wait());
/// # for (i, rsp) in (&mut iter).enumerate() {
///     // Since we used .wait(), each response is a Result.
///     match (i + 1, rsp.unwrap()) {
///         (1, "o") |
///         (2, "t") |
///         (3, "t") => {}
///         (n, i) => {
///             unreachable!("{}. response was '{}'", n, i);
///         }
///     }
/// }
///
/// // And at the end, we can get the Service back when there are no more requests.
/// let rsps = iter.into_inner();
/// assert_eq!(rsps.into_inner(), FirstLetter);
/// # }
/// ```
#[derive(Debug)]
pub struct CallAll<Svc, S>
where
    Svc: Service<S::Item>,
    S: Stream,
{
    inner: common::CallAll<Svc, S, FuturesOrdered<Svc::Future>>,
}

impl<Svc, S> CallAll<Svc, S>
where
    Svc: Service<S::Item>,
    Svc::Error: Into<Error>,
    S: Stream,
    S::Error: Into<Error>,
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
    pub fn into_inner(self) -> Svc {
        self.inner.into_inner()
    }
}

impl<Svc, S> Stream for CallAll<Svc, S>
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

impl<T: Future> common::Queue<T> for FuturesOrdered<T> {
    fn is_empty(&self) -> bool {
        FuturesOrdered::is_empty(self)
    }

    fn push(&mut self, future: T) {
        FuturesOrdered::push(self, future)
    }

    fn poll(&mut self) -> Poll<Option<T::Item>, T::Error> {
        Stream::poll(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::{ok, FutureResult};
    use futures::{Async, Poll, Stream};
    use std::cell::Cell;
    use std::rc::Rc;
    use ServiceExt;

    #[derive(Debug, Eq, PartialEq)]
    struct Srv {
        admit: Rc<Cell<bool>>,
        count: Rc<Cell<usize>>,
    }
    impl Service<&'static str> for Srv {
        type Response = &'static str;
        type Error = ();
        type Future = FutureResult<Self::Response, ()>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            if !self.admit.get() {
                return Ok(Async::NotReady);
            }

            self.admit.set(false);
            Ok(Async::Ready(()))
        }

        fn call(&mut self, req: &'static str) -> Self::Future {
            self.count.set(self.count.get() + 1);
            ok(req)
        }
    }

    #[test]
    fn test_in_order() {
        let mut mock = tokio_mock_task::MockTask::new();

        let admit = Rc::new(Cell::new(false));
        let count = Rc::new(Cell::new(0));
        let srv = Srv {
            count: count.clone(),
            admit: admit.clone(),
        };
        let (tx, rx) = futures::unsync::mpsc::unbounded();
        let mut ca = srv.call_all::<_, ()>(rx);

        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady));
        tx.unbounded_send("one").unwrap();
        mock.is_notified();
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady)); // service not admitting
        admit.set(true);
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::Ready(Some("one"))));
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady));
        admit.set(true);
        tx.unbounded_send("two").unwrap();
        mock.is_notified();
        tx.unbounded_send("three").unwrap();
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::Ready(Some("two"))));
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady)); // not yet admitted
        admit.set(true);
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::Ready(Some("three"))));
        admit.set(true);
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady)); // allowed to admit, but nothing there
        admit.set(true);
        tx.unbounded_send("four").unwrap();
        mock.is_notified();
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::Ready(Some("four"))));
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady));
        admit.set(true); // need to be ready since impl doesn't know it'll get EOF

        // When we drop the request stream, CallAll should return None.
        drop(tx);
        mock.is_notified();
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::Ready(None)));
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::Ready(None)));
        assert_eq!(count.get(), 4);

        // We should also be able to recover the wrapped Service.
        assert_eq!(
            ca.into_inner(),
            Srv {
                count: count.clone(),
                admit
            }
        );
    }
}
