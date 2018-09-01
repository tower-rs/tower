use futures::{Async, Future, Poll};
use tower_service::Service;

/// Service for the `and_then` combinator, chaining a computation onto the end of
/// another service which completes successfully.
///
/// This is created by the `ServiceExt::and_then` method.
pub struct AndThen<A, B> {
    a: A,
    b: B,
}

impl<A, B> AndThen<A, B>
where
    A: Service,
    A::Error: Into<B::Error>,
    B: Service<Request = A::Response> + Clone,
{
    /// Create new `AndThen` combinator
    pub fn new(a: A, b: B) -> AndThen<A, B> {
        AndThen { a, b }
    }
}

impl<A, B> Service for AndThen<A, B>
where
    A: Service,
    A::Error: Into<B::Error>,
    B: Service<Request = A::Response> + Clone,
{
    type Request = A::Request;
    type Response = B::Response;
    type Error = B::Error;
    type Future = AndThenFuture<A, B>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        match self.a.poll_ready() {
            Ok(Async::Ready(_)) => self.b.poll_ready(),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Err(err.into()),
        }
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        AndThenFuture::new(self.a.call(req), self.b.clone())
    }
}

pub struct AndThenFuture<A, B>
where
    A: Service,
    A::Error: Into<B::Error>,
    B: Service<Request = A::Response>,
{
    b: B,
    fut_b: Option<B::Future>,
    fut_a: A::Future,
}

impl<A, B> AndThenFuture<A, B>
where
    A: Service,
    A::Error: Into<B::Error>,
    B: Service<Request = A::Response>,
{
    fn new(fut_a: A::Future, b: B) -> Self {
        AndThenFuture {
            b,
            fut_a,
            fut_b: None,
        }
    }
}

impl<A, B> Future for AndThenFuture<A, B>
where
    A: Service,
    A::Error: Into<B::Error>,
    B: Service<Request = A::Response>,
{
    type Item = B::Response;
    type Error = B::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut fut) = self.fut_b {
            return fut.poll();
        }

        match self.fut_a.poll() {
            Ok(Async::Ready(resp)) => {
                self.fut_b = Some(self.b.call(resp));
                self.poll()
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Err(err.into()),
        }
    }
}
