use super::Error;
use futures::{try_ready, Async, Future, Poll, Stream};
use tower_service::Service;

/// TODO: Dox
#[derive(Debug)]
pub(crate) struct CallAll<Svc, S, Q> {
    service: Svc,
    stream: S,
    queue: Q,
    eof: bool,
}

pub(crate) trait Queue<T: Future> {
    fn is_empty(&self) -> bool;

    fn push(&mut self, future: T);

    fn poll(&mut self) -> Poll<Option<T::Item>, T::Error>;
}

impl<Svc, S, Q> CallAll<Svc, S, Q>
where
    Svc: Service<S::Item>,
    Svc::Error: Into<Error>,
    S: Stream,
    S::Error: Into<Error>,
    Q: Queue<Svc::Future>,
{
    pub(crate) fn new(service: Svc, stream: S, queue: Q) -> CallAll<Svc, S, Q> {
        CallAll {
            service,
            stream,
            queue,
            eof: false,
        }
    }

    /// Extract the wrapped `Service`.
    pub(crate) fn into_inner(self) -> Svc {
        self.service
    }

    pub(crate) fn unordered(self) -> super::CallAllUnordered<Svc, S> {
        assert!(self.queue.is_empty() && !self.eof);

        super::CallAllUnordered::new(self.service, self.stream)
    }
}

impl<Svc, S, Q> Stream for CallAll<Svc, S, Q>
where
    Svc: Service<S::Item>,
    Svc::Error: Into<Error>,
    S: Stream,
    S::Error: Into<Error>,
    Q: Queue<Svc::Future>,
{
    type Item = Svc::Response;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            let res = self.queue.poll().map_err(Into::into);

            // First, see if we have any responses to yield
            if let Async::Ready(Some(rsp)) = res? {
                return Ok(Async::Ready(Some(rsp)));
            }

            // If there are no more requests coming, check if we're done
            if self.eof {
                if self.queue.is_empty() {
                    return Ok(Async::Ready(None));
                } else {
                    return Ok(Async::NotReady);
                }
            }

            // Then, see that the service is ready for another request
            try_ready!(self.service.poll_ready().map_err(Into::into));

            // If it is, gather the next request (if there is one)
            match self.stream.poll().map_err(Into::into)? {
                Async::Ready(Some(req)) => {
                    self.queue.push(self.service.call(req));
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
