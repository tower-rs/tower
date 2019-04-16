use futures::{Async, Future, Poll};
use tower_service::Service;

use std::time::Duration;

/// A policy which specifies how long each request should be delayed for.
pub trait Policy<Request> {
    fn delay(&self, req: &Request) -> Duration;
}

/// A middleware which delays sending the request to the underlying service
/// for an amount of time specified by the policy.
#[derive(Debug)]
pub struct Delay<P, S> {
    policy: P,
    service: S,
}

#[derive(Debug)]
pub struct ResponseFuture<Request, S, F> {
    service: S,
    state: State<Request, F>,
}

#[derive(Debug)]
enum State<Request, F> {
    Delaying(tokio_timer::Delay, Option<Request>),
    Called(F),
}

impl<P, S> Delay<P, S> {
    pub fn new<Request>(policy: P, service: S) -> Self
    where
        P: Policy<Request>,
        S: Service<Request> + Clone,
        S::Error: Into<super::Error>,
    {
        Delay { policy, service }
    }
}

impl<Request, P, S> Service<Request> for Delay<P, S>
where
    P: Policy<Request>,
    S: Service<Request> + Clone,
    S::Error: Into<super::Error>,
{
    type Response = S::Response;
    type Error = super::Error;
    type Future = ResponseFuture<Request, S, S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service
            .poll_ready()
            .map_err(|e| e.into())
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let deadline = tokio_timer::clock::now() + self.policy.delay(&request);
        let mut cloned = self.service.clone();
        // Pass the original service to the ResponseFuture and keep the cloned service on self.
        let orig = {
            std::mem::swap(&mut cloned, &mut self.service);
            cloned
        };
        ResponseFuture {
            service: orig,
            state: State::Delaying(tokio_timer::Delay::new(deadline), Some(request)),
        }
    }
}

impl<Request, S, F> Future for ResponseFuture<Request, S, F>
where
    F: Future,
    F::Error: Into<super::Error>,
    S: Service<Request, Future = F, Response = F::Item, Error = F::Error>,
{
    type Item = F::Item;
    type Error = super::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let next = match self.state {
                State::Delaying(ref mut delay, ref mut req) => match delay.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(())) => {
                        let req = req.take().expect("Missing request in delay");
                        let fut = self.service.call(req);
                        State::Called(fut)
                    }
                    Err(e) => return Err(e.into()),
                },
                State::Called(ref mut fut) => {
                    return fut.poll().map_err(|e| e.into());
                }
            };
            self.state = next;
        }
    }
}
