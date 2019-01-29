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

impl<A, B> Then<A, B> {
    /// Create new `Then` combinator
    pub fn new<Request>(a: A, b: B) -> Then<A, B>
    where
        A: Service<Request>,
        B: Service<Result<A::Response, A::Error>, Error = A::Error> + Clone,
    {
        Then { a, b }
    }
}

impl<A, B> Clone for Then<A, B>
where
    A: Clone,
    B: Clone,
{
    fn clone(&self) -> Self {
        Then {
            a: self.a.clone(),
            b: self.b.clone(),
        }
    }
}

impl<A, B, Request> Service<Request> for Then<A, B>
where
    A: Service<Request>,
    B: Service<Result<A::Response, A::Error>, Error = A::Error> + Clone,
{
    type Response = B::Response;
    type Error = B::Error;
    type Future = ThenFuture<A, B, Request>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        let _ = try_ready!(self.a.poll_ready());
        self.b.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        ThenFuture {
            fut_a: self.a.call(req),
            b: self.b.clone(),
            fut_b: None,
        }
    }

    fn poll_service(&mut self) -> Poll<(), Self::Error> {
        let a = self.a.poll_service()?;
        let b = self.b.poll_service()?;

        if a.is_ready() && b.is_ready() {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }

    fn poll_close(&mut self) -> Poll<(), Self::Error> {
        let a = self.a.poll_close()?;
        let b = self.b.poll_close()?;

        if a.is_ready() && b.is_ready() {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }
}

pub struct ThenFuture<A, B, Request>
where
    A: Service<Request>,
    B: Service<Result<A::Response, A::Error>>,
{
    b: B,
    fut_b: Option<B::Future>,
    fut_a: A::Future,
}

impl<A, B, Request> Future for ThenFuture<A, B, Request>
where
    A: Service<Request>,
    B: Service<Result<A::Response, A::Error>>,
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

#[cfg(test)]
mod tests {
    use futures::future::{err, ok, FutureResult};
    use futures::{Async, Poll};
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use ServiceExt;

    struct Srv1(Rc<Cell<usize>>);
    impl Service<Result<&'static str, &'static str>> for Srv1 {
        type Response = &'static str;
        type Error = ();
        type Future = FutureResult<Self::Response, Self::Error>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            self.0.set(self.0.get() + 1);
            Ok(Async::Ready(()))
        }

        fn call(&mut self, req: Result<&'static str, &'static str>) -> Self::Future {
            match req {
                Ok(msg) => ok(msg),
                Err(_) => err(()),
            }
        }
    }

    #[derive(Clone)]
    struct Srv2(Rc<Cell<usize>>);

    impl Service<Result<&'static str, ()>> for Srv2 {
        type Response = (&'static str, &'static str);
        type Error = ();
        type Future = FutureResult<Self::Response, ()>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            self.0.set(self.0.get() + 1);
            Ok(Async::Ready(()))
        }

        fn call(&mut self, req: Result<&'static str, ()>) -> Self::Future {
            match req {
                Ok(msg) => ok((msg, "ok")),
                Err(()) => ok(("srv2", "err")),
            }
        }
    }

    #[test]
    fn test_poll_ready() {
        let cnt = Rc::new(Cell::new(0));
        let mut srv = Srv1(cnt.clone()).then(Srv2(cnt.clone()));
        let res = srv.poll_ready();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), Async::Ready(()));
        assert_eq!(cnt.get(), 2);
    }

    #[test]
    fn test_call() {
        let cnt = Rc::new(Cell::new(0));
        let mut srv = Srv1(cnt.clone()).then(Srv2(cnt));

        let res = srv.call(Ok("srv1")).poll();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), Async::Ready(("srv1", "ok")));

        let res = srv.call(Err("srv")).poll();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), Async::Ready(("srv2", "err")));
    }
}
