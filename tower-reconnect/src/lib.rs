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


#[macro_use]
mod macros {
    include! { concat!(env!("CARGO_MANIFEST_DIR"), "/../gen_errors.rs") }
}

kind_error!{
    #[derive(Debug)]
    pub struct Error from enum ErrorKind {
        Inner(T) => is: is_inner, into: into_inner, borrow: borrow_inner,
        Connect(U) => fmt: "error connecting", is: is_connect, into: into_connect, borrow: borrow_connect,
        NotReady => fmt: "not ready", is: is_not_ready, into: UNUSED, borrow: UNUSED
    }
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
                            ret = Err(ErrorKind::Connect(e).into());
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
                f.poll().map_err(|e| ErrorKind::Inner(e).into())
            }
            None => Err(ErrorKind::NotReady)?,
        }
    }
}
