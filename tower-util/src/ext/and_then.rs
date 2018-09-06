use futures::{Future, Poll};
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
    B: Service<Request = A::Response, Error = A::Error> + Clone,
{
    /// Create new `AndThen` combinator
    pub fn new(a: A, b: B) -> AndThen<A, B> {
        AndThen { a, b }
    }
}

impl<A, B> Clone for AndThen<A, B>
where
    A: Service + Clone,
    B: Service<Request = A::Response, Error = A::Error> + Clone,
{
    fn clone(&self) -> Self {
        AndThen {
            a: self.a.clone(),
            b: self.b.clone(),
        }
    }
}

impl<A, B> Service for AndThen<A, B>
where
    A: Service,
    B: Service<Request = A::Response, Error = A::Error> + Clone,
{
    type Request = A::Request;
    type Response = B::Response;
    type Error = B::Error;
    type Future = AndThenFuture<A, B>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let _ = try_ready!(self.a.poll_ready());
        self.b.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        AndThenFuture::new(self.a.call(req), self.b.clone())
    }
}

pub struct AndThenFuture<A, B>
where
    A: Service,
    B: Service<Request = A::Response, Error = A::Error>,
{
    b: B,
    fut_b: Option<B::Future>,
    fut_a: A::Future,
}

impl<A, B> AndThenFuture<A, B>
where
    A: Service,
    B: Service<Request = A::Response, Error = A::Error>,
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
    B: Service<Request = A::Response, Error = A::Error>,
{
    type Item = B::Response;
    type Error = B::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut fut) = self.fut_b {
            return fut.poll();
        }

        let resp = try_ready!(self.fut_a.poll());
        self.fut_b = Some(self.b.call(resp));
        self.poll()
    }
}
