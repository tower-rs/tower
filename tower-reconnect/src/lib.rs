extern crate futures;
#[macro_use]
extern crate log;
extern crate tower_service;

use futures::{Future, Async, Poll};
use tower_service::{Service, NewService};

use std::{error, fmt};

pub struct Reconnect<T>
where T: NewService,
{
    new_service: T,
    state: State<T>,
}

#[derive(Debug)]
pub struct Error<T, U> {
    kind: ErrorKind<T, U>,
}

#[derive(Debug)]
enum ErrorKind<T, U> {
    Inner(T),
    Connect(U),
    NotReady,
}

pub struct ResponseFuture<T>
where T: NewService
{
    inner: Option<<T::Service as Service>::Future>,
}

#[derive(Debug)]
enum State<T>
where T: NewService
{
    Idle,
    Connecting(T::Future),
    Connected(T::Service),
}

// ===== impl Reconnect =====

impl<T> Reconnect<T>
where T: NewService,
{
    pub fn new(new_service: T) -> Self {
        Reconnect {
            new_service,
            state: State::Idle,
        }
    }
}

impl<T> Service for Reconnect<T>
where T: NewService
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = Error<T::Error, T::InitError>;
    type Future = ResponseFuture<T>;

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
                            ret = Err(Error::connect(e));
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

    fn call(&mut self, request: Self::Request) -> Self::Future {
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

impl<T> fmt::Debug for Reconnect<T>
where T: NewService + fmt::Debug,
      T::Future: fmt::Debug,
      T::Service: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Reconnect")
            .field("new_service", &self.new_service)
            .field("state", &self.state)
            .finish()
    }
}

// ===== impl ResponseFuture =====

impl<T: NewService> Future for ResponseFuture<T> {
    type Item = T::Response;
    type Error = Error<T::Error, T::InitError>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        trace!("poll response");

        match self.inner {
            Some(ref mut f) => {
                f.poll().map_err(Error::inner)
            }
            None => Err(Error::not_ready()),
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
        match self.kind {
            ErrorKind::Inner(ref why) => fmt::Display::fmt(why, f),
            ErrorKind::Connect(ref why) => write!(f, "connection failed: {}", why),
            ErrorKind::NotReady => f.pad("not ready"),
        }
    }
}

impl<T, U> error::Error for Error<T, U>
where
    T: error::Error,
    U: error::Error,
{
    fn cause(&self) -> Option<&error::Error> {
        match self.kind {
            ErrorKind::Inner(ref why) => Some(why),
            ErrorKind::Connect(ref why) => Some(why),
            _ => None,
        }
    }

    fn description(&self) -> &str {
        match self.kind {
            ErrorKind::Inner(_) => "inner service error",
            ErrorKind::Connect(_) => "connection failed",
            ErrorKind::NotReady => "not ready",
        }
    }
}

impl<T, U> Error<T, U> {

    pub fn is_connect(&self) -> bool {
        match self.kind {
            ErrorKind::Connect(_) => true,
            _ => false,
        }
    }

    pub fn is_service(&self) -> bool {
        match self.kind {
            ErrorKind::Inner(_) => true,
            _ => false,
        }
    }

    pub fn is_not_ready(&self) -> bool {
        match self.kind {
            ErrorKind::NotReady => true,
            _ => false,
        }
    }

    pub fn into_connect(self) -> Option<U> {
        match self.kind {
            ErrorKind::Connect(e) => Some(e),
            _ => None,
        }
    }

    pub fn into_service(self) -> Option<T> {
        match self.kind {
            ErrorKind::Inner(e) => Some(e),
            _ => None,
        }
    }

    pub fn borrow_connect(&self) -> Option<&U> {
        match self.kind {
            ErrorKind::Connect(ref e) => Some(e),
            _ => None,
        }
    }

    pub fn borrow_service(&self) -> Option<&T> {
        match self.kind {
            ErrorKind::Inner(ref e) => Some(e),
            _ => None,
        }
    }

    fn inner(t: T) -> Self {
        Self {
            kind: ErrorKind::Inner(t),
        }
    }

    fn connect(c: U) -> Self {
        Self {
            kind: ErrorKind::Connect(c),
        }
    }

    fn not_ready() -> Self {
        Self {
            kind: ErrorKind::NotReady,
        }
    }
}
