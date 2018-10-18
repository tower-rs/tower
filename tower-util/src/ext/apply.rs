use std::marker::PhantomData;

use futures::{Future, IntoFuture, Poll};
use tower_service::Service;

/// `Apply` service combinator
pub struct Apply<T, F, R, Request> {
    service: T,
    f: F,
    _r: PhantomData<Fn(Request) -> R>,
}

impl<T, F, R, Request> Apply<T, F, R, Request>
where
    T: Service<Request, Error = R::Error> + Clone,
    F: Fn(Request, T) -> R,
    R: IntoFuture,
{
    /// Create new `Apply` combinator
    pub fn new(f: F, service: T) -> Self {
        Self {
            service,
            f,
            _r: PhantomData,
        }
    }
}

impl<T, F, R, Request> Clone for Apply<T, F, R, Request>
where
    T: Service<Request, Error = R::Error> + Clone,
    F: Fn(Request, T) -> R + Clone,
    R: IntoFuture,
{
    fn clone(&self) -> Self {
        Apply {
            service: self.service.clone(),
            f: self.f.clone(),
            _r: PhantomData,
        }
    }
}

impl<T, F, R, Request> Service<Request> for Apply<T, F, R, Request>
where
    T: Service<Request, Error = R::Error> + Clone,
    F: Fn(Request, T) -> R,
    R: IntoFuture,
{
    type Response = <R::Future as Future>::Item;
    type Error = <R::Future as Future>::Error;
    type Future = R::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let service = self.service.clone();
        (self.f)(req, service).into_future()
    }
}

#[cfg(test)]
mod tests {
    use futures::future::{ok, FutureResult};
    use futures::{Async, Future, Poll};
    use tower_service::Service;

    use ext::ServiceExt;

    #[derive(Clone)]
    struct Srv;
    impl Service for Srv {
        type Request = ();
        type Response = ();
        type Error = ();
        type Future = FutureResult<(), ()>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Ok(Async::Ready(()))
        }

        fn call(&mut self, _: ()) -> Self::Future {
            ok(())
        }
    }

    #[test]
    fn test_call() {
        let mut srv =
            Srv.apply(|req: &'static str, mut srv| srv.call(()).map(move |res| (req, res)));
        let res = srv.call("srv").poll();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), Async::Ready(("srv", ())));
    }
}
