use std::mem;

use futures::{Async, Future, Poll};
use tower_service::Service;

/// A `Future` consuming a `Service` and request, waiting until the `Service`
/// is ready, and then calling `Service::call` with the request, and
/// waiting for that `Future`.
pub struct Oneshot<S: Service<Req>, Req> {
    state: State<S, Req>,
}

enum State<S: Service<Req>, Req> {
    NotReady(S, Req),
    Called(S::Future),
    Tmp,
}

impl<S, Req> Oneshot<S, Req>
where
    S: Service<Req>,
{
    pub fn new(svc: S, req: Req) -> Self {
        Oneshot {
            state: State::NotReady(svc, req),
        }
    }
}

impl<S, Req> Future for Oneshot<S, Req>
where
    S: Service<Req>,
{
    type Item = S::Response;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match mem::replace(&mut self.state, State::Tmp) {
                State::NotReady(mut svc, req) => match svc.poll_ready()? {
                    Async::Ready(()) => {
                        self.state = State::Called(svc.call(req));
                    }
                    Async::NotReady => {
                        self.state = State::NotReady(svc, req);
                        return Ok(Async::NotReady);
                    }
                },
                State::Called(mut fut) => match fut.poll()? {
                    Async::Ready(res) => {
                        return Ok(Async::Ready(res));
                    }
                    Async::NotReady => {
                        self.state = State::Called(fut);
                        return Ok(Async::NotReady);
                    }
                },
                State::Tmp => panic!("polled after complete"),
            }
        }
    }
}
