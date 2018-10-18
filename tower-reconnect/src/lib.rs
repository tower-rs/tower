extern crate futures;
#[macro_use]
extern crate log;
extern crate tower_service;

use futures::{Future, Async, Poll};
use tower_service::{Service, NewService};

use std::{error, fmt};

pub struct Reconnect<T, Request>
where
    T: NewService<Request>,
{
    new_service: T,
    state: State<T, Request>,
}

#[derive(Debug)]
pub enum Error<T, U> {
    Inner(T),
    Connect(U),
    NotReady,
}

pub struct ResponseFuture<T, Request>
where
    // TODO:
    // This struct should just be generic over the response future, but
    // doing so would require changing the future's error type
    T: NewService<Request>,
{
    inner: Option<<T::Service as Service<Request>>::Future>,
}

#[derive(Debug)]
enum State<T, Request>
where
    T: NewService<Request>
{
    Idle,
    Connecting(T::Future),
    Connected(T::Service),
}

// ===== impl Reconnect =====

impl<T, Request> Reconnect<T, Request>
where
    T: NewService<Request>,
{
    pub fn new(new_service: T) -> Self {
        Reconnect {
            new_service,
            state: State::Idle,
        }
    }
}

impl<T, Request> Service<Request> for Reconnect<T, Request>
where
    T: NewService<Request>
{
    type Response = T::Response;
    type Error = Error<T::Error, T::InitError>;
    type Future = ResponseFuture<T, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        use self::State::*;

        let ret;
        let mut state;

        loop {
            match self.state {
                Idle => {
                    trace!("poll_ready; idle");
                    let fut = self.new_service.new_service();
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

impl<T, Request> fmt::Debug for Reconnect<T, Request>
where
    T: NewService<Request> + fmt::Debug,
    T::Future: fmt::Debug,
    T::Service: fmt::Debug,
    Request: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Reconnect")
            .field("new_service", &self.new_service)
            .field("state", &self.state)
            .finish()
    }
}

// ===== impl ResponseFuture =====

impl<T, Request> Future for ResponseFuture<T, Request>
where
    T: NewService<Request>,
{
    type Item = T::Response;
    type Error = Error<T::Error, T::InitError>;

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
