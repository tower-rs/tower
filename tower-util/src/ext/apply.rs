use std::marker::PhantomData;

use futures::{Future, IntoFuture, Poll};
use tower_service::Service;

/// `Apply` service combinator
pub struct Apply<T, F, In, Out, Request>
where
    T: Service<Request>,
{
    service: T,
    f: F,
    _r: PhantomData<Fn((In, Request)) -> Out>,
}

impl<T, F, In, Out, Request> Apply<T, F, In, Out, Request>
where
    T: Service<Request>,
    F: Fn(In, T) -> Out,
    Out: IntoFuture,
{
    /// Create new `Apply` combinator
    pub(crate) fn new(f: F, service: T) -> Self {
        Self {
            service,
            f,
            _r: PhantomData,
        }
    }
}

impl<T, F, In, Out, Request> Clone for Apply<T, F, In, Out, Request>
where
    T: Service<Request> + Clone,
    F: Clone,
{
    fn clone(&self) -> Self {
        Apply {
            service: self.service.clone(),
            f: self.f.clone(),
            _r: PhantomData,
        }
    }
}

impl<T, F, In, Out, Request> Service<In> for Apply<T, F, In, Out, Request>
where
    T: Service<Request, Error = Out::Error> + Clone,
    F: Fn(In, T) -> Out,
    Out: IntoFuture,
{
    type Response = <Out::Future as Future>::Item;
    type Error = <Out::Future as Future>::Error;
    type Future = Out::Future;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: In) -> Self::Future {
        let service = self.service.clone();
        (self.f)(req, service).into_future()
    }

    fn poll_service(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_service()
    }

    fn poll_close(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_close()
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
    impl Service<()> for Srv {
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
