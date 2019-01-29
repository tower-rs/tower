use std::marker::PhantomData;

use futures::{Future, Poll};
use tower_service::Service;

/// Service for the `from_err` combinator, changing the error type of a service.
///
/// This is created by the `ServiceExt::from_err` method.
pub struct FromErr<A, E> {
    service: A,
    _e: PhantomData<E>,
}

impl<A, E> FromErr<A, E> {
    pub(crate) fn new<Request>(service: A) -> Self
    where
        A: Service<Request>,
        E: From<A::Error>,
    {
        FromErr {
            service,
            _e: PhantomData,
        }
    }
}

impl<A, E> Clone for FromErr<A, E>
where
    A: Clone,
{
    fn clone(&self) -> Self {
        FromErr {
            service: self.service.clone(),
            _e: PhantomData,
        }
    }
}

impl<A, E, Request> Service<Request> for FromErr<A, E>
where
    A: Service<Request>,
    E: From<A::Error>,
{
    type Response = A::Response;
    type Error = E;
    type Future = FromErrFuture<A::Future, E>;

    fn poll_ready(&mut self) -> Poll<(), E> {
        Ok(self.service.poll_ready().map_err(E::from)?)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        FromErrFuture {
            fut: self.service.call(req),
            f: PhantomData,
        }
    }


    fn poll_service(&mut self) -> Poll<(), Self::Error> {
        let res = self.service.poll_service()?;
        Ok(res)
    }

    fn poll_close(&mut self) -> Poll<(), Self::Error> {
        let res = self.service.poll_close()?;
        Ok(res)
    }
}

pub struct FromErrFuture<A, E> {
    fut: A,
    f: PhantomData<E>,
}

impl<A, E> Future for FromErrFuture<A, E>
where
    A: Future,
    E: From<A::Error>,
{
    type Item = A::Item;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.fut.poll().map_err(E::from)
    }
}

#[cfg(test)]
mod tests {
    use futures::future::{err, FutureResult};

    use super::*;
    use ServiceExt;

    struct Srv;

    impl Service<()> for Srv {
        type Response = ();
        type Error = ();
        type Future = FutureResult<(), ()>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Err(())
        }

        fn call(&mut self, _: ()) -> Self::Future {
            err(())
        }
    }

    #[derive(Debug, PartialEq)]
    struct Error;

    impl From<()> for Error {
        fn from(_: ()) -> Self {
            Error
        }
    }

    #[test]
    fn test_poll_ready() {
        let mut srv = Srv.from_err::<Error>();
        let res = srv.poll_ready();
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), Error);
    }

    #[test]
    fn test_call() {
        let mut srv = Srv.from_err::<Error>();
        let res = srv.call(()).poll();
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), Error);
    }
}
