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
pub trait Recognize<Request>: 'static {
    /// Inner service's response
    type Response;

    /// Error produced by a failed inner service request
    type Error;

    /// Error produced by failed route recognition
    type RouteError;

    /// The destination service
    type Service: Service<Request,
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
    fn recognize(&mut self, request: &Request)
        -> Result<&mut Self::Service, Self::RouteError>;
}

pub struct ResponseFuture<T, Request>
where T: Recognize<Request>,
{
    state: ResponseState<T, Request>,
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

enum ResponseState<T, Request>
where T: Recognize<Request>
{
    Dispatched(<T::Service as Service<Request>>::Future),
    RouteError(T::RouteError),
    Queued {
        service: BorrowGuard<T::Service>,
        request: Request,
    },
    NotReady,
    Invalid,
}

// ===== impl Router =====

impl<T> Router<T> {
    /// Create a new router
    pub fn new<Request>(recognize: T) -> Self
    where
        T: Recognize<Request>,
    {
        Router { recognize: Borrow::new(recognize) }
    }
}

impl<T, Request> Service<Request> for Router<T>
where T: Recognize<Request>,
{
    type Response = T::Response;
    type Error = Error<T::Error, T::RouteError>;
    type Future = ResponseFuture<T, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // Checks if there is an outstanding borrow (i.e. there is an in-flight
        // request that is blocked on an inner service).
        //
        // Borrow::poll_ready returning an error means the borrow was poisoned.
        // A panic is fine.
        self.recognize.poll_ready().map_err(|_| panic!())
    }

    fn call(&mut self, request: Request) -> Self::Future {
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

impl<T, Request> Future for ResponseFuture<T, Request>
where T: Recognize<Request>,
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
