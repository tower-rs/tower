use futures::{Future, Poll};
use tower_service::Service;

/// Service for the `and_then` combinator, chaining a computation onto the end of
/// another service which completes successfully.
///
/// This is created by the `ServiceExt::and_then` method.
#[derive(Clone)]
pub struct AndThen<A, B> {
    a: A,
    b: B,
}

impl<A, B> AndThen<A, B> {
    /// Create new `AndThen` combinator
    pub fn new<Request>(a: A, b: B) -> AndThen<A, B>
    where
        A: Service<Request>,
        B: Service<A::Response, Error = A::Error> + Clone,
    {
        AndThen { a, b }
    }
}

impl<A, B, Request> Service<Request> for AndThen<A, B>
where
    A: Service<Request>,
    B: Service<A::Response, Error = A::Error> + Clone,
{
    type Response = B::Response;
    type Error = B::Error;
    type Future = AndThenFuture<A, B, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let _ = try_ready!(self.a.poll_ready());
        self.b.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        AndThenFuture {
            fut_a: self.a.call(req),
            b: self.b.clone(),
            fut_b: None,
        }
    }
}

pub struct AndThenFuture<A, B, Request>
where
    A: Service<Request>,
    B: Service<A::Response, Error = A::Error>,
{
    b: B,
    fut_b: Option<B::Future>,
    fut_a: A::Future,
}

impl<A, B, Request> Future for AndThenFuture<A, B, Request>
where
    A: Service<Request>,
    B: Service<A::Response, Error = A::Error>,
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

#[cfg(test)]
mod tests {
    use futures::future::{ok, FutureResult};
    use futures::{Async, Poll};
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use ServiceExt;

    struct Srv1(Rc<Cell<usize>>);
    impl Service for Srv1 {
        type Request = &'static str;
        type Response = &'static str;
        type Error = ();
        type Future = FutureResult<Self::Response, ()>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            self.0.set(self.0.get() + 1);
            Ok(Async::Ready(()))
        }

        fn call(&mut self, req: Self::Request) -> Self::Future {
            ok(req)
        }
    }

    #[derive(Clone)]
    struct Srv2(Rc<Cell<usize>>);

    impl Service for Srv2 {
        type Request = &'static str;
        type Response = (&'static str, &'static str);
        type Error = ();
        type Future = FutureResult<Self::Response, ()>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            self.0.set(self.0.get() + 1);
            Ok(Async::Ready(()))
        }

        fn call(&mut self, req: Self::Request) -> Self::Future {
            ok((req, "srv2"))
        }
    }

    #[test]
    fn test_poll_ready() {
        let cnt = Rc::new(Cell::new(0));
        let mut srv = Srv1(cnt.clone()).and_then(Srv2(cnt.clone()));
        let res = srv.poll_ready();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), Async::Ready(()));
        assert_eq!(cnt.get(), 2);
    }

    #[test]
    fn test_call() {
        let cnt = Rc::new(Cell::new(0));
        let mut srv = Srv1(cnt.clone()).and_then(Srv2(cnt));
        let res = srv.call("srv1").poll();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), Async::Ready(("srv1", "srv2")));
    }
}
