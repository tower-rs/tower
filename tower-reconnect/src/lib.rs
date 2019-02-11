extern crate futures;
#[macro_use]
extern crate log;
extern crate tower_service;
extern crate tower_util;

use futures::{Async, Future, Poll};
use tower_service::Service;
use tower_util::MakeService;

use std::{error::Error as StdError, fmt};

pub struct Reconnect<M, Target>
where
    M: Service<Target>,
{
    mk_service: M,
    state: State<M::Future, M::Response>,
    target: Target,
}

#[derive(Debug)]
pub struct NotReadyError;

impl StdError for NotReadyError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self)
    }
}

impl fmt::Display for NotReadyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Service not ready")
    }
}

type Error = Box<StdError + Send + Sync>;

pub struct ResponseFuture<F> {
    inner: Option<F>,
}

#[derive(Debug)]
enum State<F, S> {
    Idle,
    Connecting(F),
    Connected(S),
}

// ===== impl Reconnect =====

impl<M, Target> Reconnect<M, Target>
where
    M: Service<Target>,
    Target: Clone,
{
    pub fn new(mk_service: M, target: Target) -> Self {
        Reconnect {
            mk_service,
            state: State::Idle,
            target,
        }
    }
}

impl<M, Target, S, Request> Service<Request> for Reconnect<M, Target>
where
    M: Service<Target, Response = S>,
    M::Error: Into<Error>,
    S: Service<Request>,
    S::Error: Into<Error>,
    Target: Clone,
{
    type Response = S::Response;
    type Error = Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let ret;
        let mut state;

        loop {
            match self.state {
                State::Idle => {
                    trace!("poll_ready; idle");
                    match self.mk_service.poll_ready() {
                        Ok(Async::Ready(())) => (),
                        Ok(Async::NotReady) => {
                            trace!("poll_ready; MakeService not ready");
                            return Ok(Async::NotReady);
                        }
                        Err(e) => {
                            trace!("poll_ready; MakeService error");
                            return Err(e.into());
                        }
                    }

                    let fut = self.mk_service.make_service(self.target.clone());
                    self.state = State::Connecting(fut);
                    continue;
                }
                State::Connecting(ref mut f) => {
                    trace!("poll_ready; connecting");
                    match f.poll() {
                        Ok(Async::Ready(service)) => {
                            state = State::Connected(service);
                        }
                        Ok(Async::NotReady) => {
                            trace!("poll_ready; not ready");
                            return Ok(Async::NotReady);
                        }
                        Err(e) => {
                            trace!("poll_ready; error");
                            state = State::Idle;
                            ret = Err(e.into());
                            break;
                        }
                    }
                }
                State::Connected(ref mut inner) => {
                    trace!("poll_ready; connected");
                    match inner.poll_ready() {
                        Ok(Async::Ready(_)) => {
                            trace!("poll_ready; ready");
                            return Ok(Async::Ready(()));
                        }
                        Ok(Async::NotReady) => {
                            trace!("poll_ready; not ready");
                            return Ok(Async::NotReady);
                        }
                        Err(_) => {
                            trace!("poll_ready; error");
                            state = State::Idle;
                        }
                    }
                }
            }

            self.state = state;
        }

        self.state = state;
        ret
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let service = match self.state {
            State::Connected(ref mut service) => service,
            _ => return ResponseFuture::new(None),
        };

        let fut = service.call(request);
        ResponseFuture::new(Some(fut))
    }
}

impl<M, Target> fmt::Debug for Reconnect<M, Target>
where
    M: Service<Target> + fmt::Debug,
    M::Future: fmt::Debug,
    M::Response: fmt::Debug,
    Target: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Reconnect")
            .field("mk_service", &self.mk_service)
            .field("state", &self.state)
            .field("target", &self.target)
            .finish()
    }
}

// ===== impl ResponseFuture =====

impl<F> ResponseFuture<F> {
    fn new(inner: Option<F>) -> Self {
        ResponseFuture { inner }
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
        match self.inner {
            Some(ref mut f) => f.poll().map_err(|e| e.into()),
            None => Err(NotReadyError.into()),
        }
    }
}
