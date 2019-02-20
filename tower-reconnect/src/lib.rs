extern crate futures;
#[macro_use]
extern crate log;
extern crate tower_service;
extern crate tower_util;

use futures::{Async, Future, Poll};
use tower_service::Service;
use tower_util::MakeService;

use std::{error, fmt, marker::PhantomData};

pub struct Reconnect<M, Target>
where
    M: Service<Target>,
{
    mk_service: M,
    state: State<M::Future, M::Response>,
    target: Target,
}

#[derive(Debug)]
pub enum Error<T, U> {
    Service(T),
    Connect(U),
}

pub struct ResponseFuture<F, E> {
    inner: F,
    _connect_error_marker: PhantomData<fn() -> E>,
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
    S: Service<Request>,
    Target: Clone,
{
    type Response = S::Response;
    type Error = Error<S::Error, M::Error>;
    type Future = ResponseFuture<S::Future, M::Error>;

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
                            return Err(Error::Connect(e));
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
                            ret = Err(Error::Connect(e));
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
            _ => panic!("service not ready; poll_ready must be called first"),
        };

        let fut = service.call(request);
        ResponseFuture::new(fut)
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

impl<F, E> ResponseFuture<F, E> {
    fn new(inner: F) -> Self {
        ResponseFuture {
            inner,
            _connect_error_marker: PhantomData,
        }
    }
}

impl<F, E> Future for ResponseFuture<F, E>
where
    F: Future,
{
    type Item = F::Item;
    type Error = Error<F::Error, E>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll().map_err(Error::Service)
    }
}

// ===== impl Error =====

impl<T, U> fmt::Display for Error<T, U>
where
    T: fmt::Display,
    U: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Service(ref why) => fmt::Display::fmt(why, f),
            Error::Connect(ref why) => write!(f, "connection failed: {}", why),
        }
    }
}

impl<T, U> error::Error for Error<T, U>
where
    T: error::Error,
    U: error::Error,
{
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::Service(ref why) => Some(why),
            Error::Connect(ref why) => Some(why),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Service(_) => "inner service error",
            Error::Connect(_) => "connection failed",
        }
    }
}
