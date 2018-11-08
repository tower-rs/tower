extern crate futures;
#[macro_use]
extern crate log;
extern crate tower_service;

use futures::{Future, Async, Poll};
use tower_service::{Service, MakeService};

use std::{error, fmt};

pub struct Reconnect<T, Target, Request>
where
    T: MakeService<Target, Request>,
{
    context: Target,
    mk_service: T,
    state: State<T::Future, T::Service>,
}

#[derive(Debug)]
pub enum Error<T, U> {
    Inner(T),
    Connect(U),
    NotReady,
}

pub struct ResponseFuture<T, Target, Request>
where
    // TODO:
    // This struct should just be generic over the response future, but
    // doing so would require changing the future's error type
    T: MakeService<Target, Request>,
{
    inner: Option<<T::Service as Service<Request>>::Future>,
}

#[derive(Debug)]
enum State<F, S> {
    Idle,
    Connecting(F),
    Connected(S),
}

// ===== impl Reconnect =====

impl<T, Target, Request> Reconnect<T, Target, Request>
where
    T: MakeService<Target, Request>,
    Target: Clone,
{
    pub fn new(mk_service: T, context: Target) -> Self {
        Reconnect {
            context,
            mk_service,
            state: State::Idle,
        }
    }
}

impl<T, Target, Request> Service<Request> for Reconnect<T, Target, Request>
where
    T: MakeService<Target, Request>,
    Target: Clone,
{
    type Response = T::Response;
    type Error = Error<T::Error, T::MakeError>;
    type Future = ResponseFuture<T, Target, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        use self::State::*;

        let ret;
        let mut state;

        loop {
            match self.state {
                Idle => {
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

                    let fut = self.mk_service.make_service(self.context.clone());
                    self.state = Connecting(fut);
                    continue;
                }
                Connecting(ref mut f) => {
                    trace!("poll_ready; connecting");
                    match f.poll() {
                        Ok(Async::Ready(service)) => {
                            state = Connected(service);
                        }
                        Ok(Async::NotReady) => {
                            trace!("poll_ready; not ready");
                            return Ok(Async::NotReady);
                        }
                        Err(e) => {
                            trace!("poll_ready; error");
                            state = Idle;
                            ret = Err(Error::Connect(e));
                            break;
                        }
                    }
                }
                Connected(ref mut inner) => {
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
                            state = Idle;
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
        use self::State::*;

        trace!("call");

        let service = match self.state {
            Connected(ref mut service) => service,
            _ => return ResponseFuture { inner: None },
        };

        let fut = service.call(request);
        ResponseFuture { inner: Some(fut) }
    }
}

impl<T, Target, Request> fmt::Debug for Reconnect<T, Target, Request>
where
    T: MakeService<Target, Request> + fmt::Debug,
    T::Future: fmt::Debug,
    T::Service: fmt::Debug,
    Target: fmt::Debug,
    Request: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Reconnect")
            .field("context", &self.context)
            .field("mk_service", &self.mk_service)
            .field("state", &self.state)
            .finish()
    }
}

// ===== impl ResponseFuture =====

impl<T, Target, Request> Future for ResponseFuture<T, Target, Request>
where
    T: MakeService<Target, Request>,
{
    type Item = T::Response;
    type Error = Error<T::Error, T::MakeError>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        trace!("poll response");

        match self.inner {
            Some(ref mut f) => {
                f.poll().map_err(Error::Inner)
            }
            None => Err(Error::NotReady),
        }
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
            Error::Inner(ref why) => fmt::Display::fmt(why, f),
            Error::Connect(ref why) => write!(f, "connection failed: {}", why),
            Error::NotReady => f.pad("not ready"),
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
            Error::Inner(ref why) => Some(why),
            Error::Connect(ref why) => Some(why),
            _ => None,
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::Inner(_) => "inner service error",
            Error::Connect(_) => "connection failed",
            Error::NotReady => "not ready",
        }
    }
}
