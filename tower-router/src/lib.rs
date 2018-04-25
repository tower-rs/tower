//! Routes requests to one of many inner inner services based on the request.

extern crate tower_service;

#[macro_use]
extern crate futures;
extern crate futures_borrow;

use tower_service::Service;

use futures::{Future, Poll};
use futures_borrow::{Borrow, BorrowGuard};

use std::mem;

use self::ResponseState::*;

/// Routes requests to an inner service based on the request.
pub struct Router<T> {
    recognize: Borrow<T>,
}

/// Matches the request with a route
pub trait Recognize: 'static {
    /// Request being matched
    type Request;

    /// Inner service's response
    type Response;

    /// Error produced by a failed inner service request
    type Error;

    /// Error produced by failed route recognition
    type RouteError;

    /// The destination service
    type Service: Service<Request = Self::Request,
                         Response = Self::Response,
                            Error = Self::Error>;

    /// Recognize a route
    ///
    /// Takes a request, returns the route matching the request.
    ///
    /// The returned value is a mutable reference to the destination `Service`.
    /// However, it may be that some asynchronous initialization must be
    /// performed before the service is able to process requests (for example,
    /// a TCP connection might need to be established). In this case, the inner
    /// service should determine the buffering strategy used to handle the
    /// request until the request can be processed.  This behavior enables
    /// punting all buffering decisions to the inner service.
    fn recognize(&mut self, request: &Self::Request)
        -> Result<&mut Self::Service, Self::RouteError>;
}

pub struct ResponseFuture<T>
where T: Recognize,
{
    state: ResponseState<T>,
}

/// Error produced by the `Router` service
///
/// TODO: Make variants priv
#[derive(Debug)]
pub enum Error<T, U> {
    /// Error produced by inner service.
    Inner(T),

    /// Error produced during route recognition.
    Route(U),

    /// Request sent when not ready.
    NotReady,
}

enum ResponseState<T>
where T: Recognize
{
    Dispatched(<T::Service as Service>::Future),
    RouteError(T::RouteError),
    Queued {
        service: BorrowGuard<T::Service>,
        request: T::Request,
    },
    NotReady,
    Invalid,
}

// ===== impl Router =====

impl<T> Router<T>
where T: Recognize
{
    /// Create a new router
    pub fn new(recognize: T) -> Self {
        Router { recognize: Borrow::new(recognize) }
    }
}

impl<T> Service for Router<T>
where T: Recognize,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = Error<T::Error, T::RouteError>;
    type Future = ResponseFuture<T>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // Checks if there is an outstanding borrow (i.e. there is an in-flight
        // request that is blocked on an inner service).
        //
        // Borrow::poll_ready returning an error means the borrow was poisoned.
        // A panic is fine.
        self.recognize.poll_ready().map_err(|_| panic!())
    }

    fn call(&mut self, request: Self::Request) -> Self::Future {
        let borrow = match self.recognize.try_borrow() {
            Ok(borrow) => borrow,
            Err(_) => {
                return ResponseFuture {
                    state: NotReady,
                }
            }
        };

        let recognize = Borrow::try_map(borrow, |recognize| {
            // Match the service
            recognize.recognize(&request)
        });

        match recognize {
            Ok(service) => {
                ResponseFuture {
                    state: Queued {
                        service,
                        request,
                    },
                }
            }
            Err((_, err)) => {
                ResponseFuture {
                    state: RouteError(err),
                }
            }
        }
    }
}

// ===== impl ResponseFuture =====

impl<T> Future for ResponseFuture<T>
where T: Recognize,
{
    type Item = T::Response;
    type Error = Error<T::Error, T::RouteError>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.state {
                Dispatched(ref mut inner) => {
                    return inner.poll()
                        .map_err(Error::Inner);
                }
                Queued { ref mut service, .. } => {
                    let res = service.poll_ready()
                        .map_err(Error::Inner);

                    try_ready!(res);

                    // Fall through to transition state
                }
                _ => {}
            }

            match mem::replace(&mut self.state, Invalid) {
                Dispatched(..) => unreachable!(),
                Queued { mut service, request } => {
                    let response = service.call(request);
                    self.state = Dispatched(response);
                }
                RouteError(err) => {
                    return Err(Error::Route(err));
                }
                NotReady => {
                    return Err(Error::NotReady);
                }
                Invalid => panic!(),
            }
        }
    }
}
