//! Future types

use crate::error::{self, Error};
use futures::{Async, Future, Poll};
use tower_service::Service;

/// Filtered response future
#[derive(Debug)]
pub struct ResponseFuture<T, S, Request>
where
    S: Service<Request>,
{
    /// Response future state
    state: State<Request, S::Future>,

    /// Predicate future
    check: T,

    /// Inner service
    service: S,
}

#[derive(Debug)]
enum State<Request, U> {
    Check(Request),
    WaitResponse(U),
    Invalid,
}

impl<T, S, Request> ResponseFuture<T, S, Request>
where
    T: Future<Error = Error>,
    S: Service<Request>,
    S::Error: Into<error::Source>,
{
    pub(crate) fn new(request: Request, check: T, service: S) -> Self {
        ResponseFuture {
            state: State::Check(request),
            check,
            service,
        }
    }
}

impl<T, S, Request> Future for ResponseFuture<T, S, Request>
where
    T: Future<Error = Error>,
    S: Service<Request>,
    S::Error: Into<error::Source>,
{
    type Item = S::Response;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::State::*;
        use std::mem;

        loop {
            match mem::replace(&mut self.state, Invalid) {
                Check(request) => {
                    // Poll predicate
                    match self.check.poll()? {
                        Async::Ready(_) => {
                            let response = self.service.call(request);
                            self.state = WaitResponse(response);
                        }
                        Async::NotReady => {
                            self.state = Check(request);
                            return Ok(Async::NotReady);
                        }
                    }
                }
                WaitResponse(mut response) => {
                    let ret = response.poll().map_err(Error::inner);

                    self.state = WaitResponse(response);

                    return ret;
                }
                Invalid => {
                    panic!("invalid state");
                }
            }
        }
    }
}
