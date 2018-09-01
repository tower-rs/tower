use futures::{Async, Future, Poll};
use tower_service::Service;

/// Service for the `then` combinator, chaining a computation onto the end of
/// another service.
///
/// This is created by the `ServiceExt::then` method.
pub struct Then<A, B> {
    a: A,
    b: B,
}

impl<A, B> Then<A, B>
where
    A: Service,
    A::Error: Into<B::Error>,
    B: Service<Request = Result<A::Response, A::Error>> + Clone,
{
    /// Create new `Then` combinator
    pub fn new(a: A, b: B) -> Then<A, B> {
        Then { a, b }
    }
}

impl<A, B> Service for Then<A, B>
where
    A: Service,
    A::Error: Into<B::Error>,
    B: Service<Request = Result<A::Response, A::Error>> + Clone,
{
    type Request = A::Request;
    type Response = B::Response;
    type Error = B::Error;
    type Future = ThenFuture<A, B>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        match self.a.poll_ready() {
            Ok(Async::Ready(_)) => self.b.poll_ready(),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => Err(err.into()),
        }
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        ThenFuture::new(self.a.call(req), self.b.clone())
    }
}

pub struct ThenFuture<A, B>
where
    A: Service,
    B: Service<Request = Result<A::Response, A::Error>>,
{
    b: B,
    fut_b: Option<B::Future>,
    fut_a: A::Future,
}

impl<A, B> ThenFuture<A, B>
where
    A: Service,
    B: Service<Request = Result<A::Response, A::Error>>,
{
    fn new(fut_a: A::Future, b: B) -> Self {
        ThenFuture {
            b,
            fut_a,
            fut_b: None,
        }
    }
}

impl<A, B> Future for ThenFuture<A, B>
where
    A: Service,
    B: Service<Request = Result<A::Response, A::Error>>,
{
    type Item = B::Response;
    type Error = B::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut fut) = self.fut_b {
            return fut.poll();
        }

        match self.fut_a.poll() {
            Ok(Async::Ready(resp)) => {
                self.fut_b = Some(self.b.call(Ok(resp)));
                self.poll()
            }
            Err(err) => {
                self.fut_b = Some(self.b.call(Err(err)));
                self.poll()
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
        }
    }
}
