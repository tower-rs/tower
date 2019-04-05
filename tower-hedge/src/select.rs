use futures::{Async, Future, Poll};
use tower_service::Service;

/// A policy which decides which requests can be cloned and sent to the B
/// service.
pub trait Policy<Request> {
    fn clone_request(&self, req: &Request) -> Option<Request>;
}

/// Select is a middleware which attempts to clone the request and sends the
/// original request to the A service and, if the request was able to be cloned,
/// the cloned request to the B service.  Both resulting futures will be polled
/// and whichever future completes first will be used as the result.
pub struct Select<P, A, B> {
    policy: P,
    a: A,
    b: B
}

pub struct ResponseFuture<AF, BF> {
    a_fut: AF,
    b_fut: Option<BF>,
}

impl<P, A, B> Select<P, A, B>
{
    pub fn new<Request>(policy: P, a: A, b: B) -> Self
    where
        P: Policy<Request>,
        A: Service<Request>,
        A::Error: Into<super::Error>,
        B: Service<Request, Response = A::Response>,
        B::Error: Into<super::Error>,
    {
        Select { policy, a, b }
    }
}

impl<P, A, B, Request> Service<Request> for Select<P, A, B>
where
    P: Policy<Request>,
    A: Service<Request>,
    A::Error: Into<super::Error>,
    B: Service<Request, Response = A::Response>,
    B::Error: Into<super::Error>,
    
{
    type Response = A::Response;
    type Error = super::Error;
    type Future = ResponseFuture<A::Future, B::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let a = self.a.poll_ready().map_err(|e| e.into())?;
        let b = self.b.poll_ready().map_err(|e| e.into())?;
        if a.is_ready() && b.is_ready() {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let b_fut = if let Some(cloned_req) = self.policy.clone_request(&request) {
            Some(self.b.call(cloned_req))
        } else {
            None
        };
        ResponseFuture {
            a_fut: self.a.call(request),
            b_fut,
        }
    }
}

impl<AF, BF> Future for ResponseFuture<AF, BF> 
where
    AF: Future,
    AF::Error: Into<super::Error>,
    BF: Future<Item = AF::Item>,
    BF::Error: Into<super::Error>,
{
    type Item = AF::Item;
    type Error = super::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.a_fut.poll() {
            Ok(Async::NotReady) => {},
            Ok(Async::Ready(a)) => return Ok(Async::Ready(a)),
            Err(e) => return Err(e.into()),
        }
        if let Some(ref mut b_fut) = self.b_fut {
            match b_fut.poll() {
                Ok(Async::NotReady) => {},
                Ok(Async::Ready(b)) => return Ok(Async::Ready(b)),
                Err(e) => return Err(e.into()),
            }
        }
        return Ok(Async::NotReady)
    }
}
