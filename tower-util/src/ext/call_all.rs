//! `Stream<Item = Request>` + `Service<Request>` => `Stream<Item = Response>`.

use futures::stream::FuturesOrdered;
use futures::{try_ready, Async, Poll, Stream};
use std::marker::PhantomData;
use tower_service::Service;

/// Items yielded by `CallAll`'s implementation of `Stream`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum StateStreamItem<S, Response> {
    /// For every submitted request, `CallAll` yields an `Item` with the corresponding response.
    Item(Response),
    /// When the incoming request `Stream` has ended, `CallAll` yields the wrapped service in this.
    Service(S),
}

/// This is a `futures::Stream` of responses resulting from calling the wrapped `tower::Service`
/// for each request received on the wrapped `Stream`.
///
/// ```rust
/// # extern crate futures;
/// # extern crate tower_service;
/// # extern crate tokio_mock_task;
/// # extern crate tower_util;
/// # use futures::future::{ok, FutureResult};
/// # use futures::{Async, Poll};
/// # use std::cell::Cell;
/// # use std::rc::Rc;
/// #
/// use futures::Stream;
/// use tower_service::Service;
/// use tower_util::ServiceExt;
/// use tower_util::ext::StateStreamItem;
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
/// for (i, rsp) in rsps.wait().enumerate() {
/// # */
/// # for (i, rsp) in mock.enter(|| rsps.wait()).enumerate() {
///     // Since we used .wait(), each response is a Result.
///     match (i + 1, rsp.unwrap()) {
///         // We either get an Item if this is a response to a request.
///         (1, StateStreamItem::Item("o")) |
///         (2, StateStreamItem::Item("t")) |
///         (3, StateStreamItem::Item("t")) => {}
///         (n, StateStreamItem::Item(i)) => {
///             unreachable!("{}. response was '{}'", n, i);
///         }
///         // Or we get the Service back when there are no more requests.
///         (n, StateStreamItem::Service(s)) => {
///             assert_eq!(n, 4);
///             assert_eq!(s, FirstLetter);
///         }
///     }
/// }
/// # }
/// ```
#[derive(Debug)]
pub struct CallAll<Svc, S, E>
where
    Svc: Service<S::Item>,
    S: Stream,
{
    // Will be Some until S is exhausten, then it will be yielded.
    svc: Option<Svc>,
    stream: S,
    responses: FuturesOrdered<Svc::Future>,
    eof: bool,
    error: PhantomData<E>,
}

impl<Svc, S, E> CallAll<Svc, S, E>
where
    Svc: Service<S::Item>,
    S: Stream,
{
    /// Create new `CallAll` combinator.
    ///
    /// Each request yielded by `stread` is passed to `svc`, and the resulting responses are
    /// yielded in the same order by the implementation of `Stream` for `CallAll`.
    pub fn new(svc: Svc, stream: S) -> CallAll<Svc, S, E> {
        CallAll {
            svc: Some(svc),
            stream,
            responses: FuturesOrdered::new(),
            eof: false,
            error: PhantomData,
        }
    }
}

impl<Svc, S, E> Stream for CallAll<Svc, S, E>
where
    Svc: Service<S::Item>,
    S: Stream,
    E: From<Svc::Error>,
    E: From<S::Error>,
{
    type Item = StateStreamItem<Svc, Svc::Response>;
    type Error = E;
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            // If the service has been taken, we know we're done.
            if self.svc.is_none() {
                return Ok(Async::Ready(None));
            }

            // First, see if we have any responses to yield
            if let Async::Ready(Some(rsp)) = self.responses.poll()? {
                return Ok(Async::Ready(Some(StateStreamItem::Item(rsp))));
            }

            // If there are no more requests coming, check if we're done
            if self.eof {
                if self.responses.is_empty() {
                    return Ok(Async::Ready(self.svc.take().map(StateStreamItem::Service)));
                } else {
                    return Ok(Async::NotReady);
                }
            }

            // We check for svc.is_none() above.
            let svc = self.svc.as_mut().unwrap();

            // Then, see that the service is ready for another request
            try_ready!(svc.poll_ready());

            // If it is, gather the next request (if there is one)
            match self.stream.poll()? {
                Async::Ready(Some(req)) => {
                    self.responses.push(svc.call(req));
                }
                Async::Ready(None) => {
                    // We're all done once any outstanding requests have completed
                    self.eof = true;
                }
                Async::NotReady => {
                    // TODO: We probably want to "release" the slot we reserved in Svc here.
                    // It may be a while until we get around to actually using it.
                }
            }
        }
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
        assert_eq!(
            mock.enter(|| ca.poll()),
            Ok(Async::Ready(Some(StateStreamItem::Item("one"))))
        );
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady));
        admit.set(true);
        tx.unbounded_send("two").unwrap();
        mock.is_notified();
        tx.unbounded_send("three").unwrap();
        assert_eq!(
            mock.enter(|| ca.poll()),
            Ok(Async::Ready(Some(StateStreamItem::Item("two"))))
        );
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady)); // not yet admitted
        admit.set(true);
        assert_eq!(
            mock.enter(|| ca.poll()),
            Ok(Async::Ready(Some(StateStreamItem::Item("three"))))
        );
        admit.set(true);
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady)); // allowed to admit, but nothing there
        admit.set(true);
        tx.unbounded_send("four").unwrap();
        mock.is_notified();
        assert_eq!(
            mock.enter(|| ca.poll()),
            Ok(Async::Ready(Some(StateStreamItem::Item("four"))))
        );
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::NotReady));
        admit.set(true); // need to be ready since impl doesn't know it'll get EOF

        // When we drop the request stream, CallAll should return the wrapped Service.
        drop(tx);
        mock.is_notified();
        assert_eq!(
            mock.enter(|| ca.poll()),
            Ok(Async::Ready(Some(StateStreamItem::Service(Srv {
                count: count.clone(),
                admit
            }))))
        );

        // And from that point onwards it should return None.
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::Ready(None)));
        assert_eq!(mock.enter(|| ca.poll()), Ok(Async::Ready(None)));
        assert_eq!(count.get(), 4);
    }
}
