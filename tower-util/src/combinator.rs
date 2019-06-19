use futures::{Future, Poll};
use tower_service::Service;

pub struct Map<T, F> {
    inner: T,
    f: F,
}

impl<T, F> Map<T, F> {
    pub fn new(service: T, f: F) -> Self {
        Self {
            inner: service,
            f,
        }
    }
}

impl<Request, T, F, U> Service<Request> for Map<T, F>
where
    T: Service<Request>,
    F: Fn(T::Response) -> U + Copy,
{
    type Response = U;
    type Error = T::Error;
    type Future = futures::future::Map<T::Future, F>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let f = self.f.clone();
        self.inner.call(req).map(f)
    }
}

pub struct MapErr<T, F> {
    inner: T,
    f: F,
}

impl<T, F> MapErr<T, F> {
    pub fn new(service: T, f: F) -> Self {
        Self {
            inner: service,
            f,
        }
    }
}

impl<Request, T, F, U> Service<Request> for MapErr<T, F>
where
    T: Service<Request>,
    F: Fn(T::Error) -> U + Copy,
{
    type Response = T::Response;
    type Error = U;
    type Future = futures::future::MapErr<T::Future, F>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(|e| (self.f)(e))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let f = self.f.clone();
        self.inner.call(req).map_err(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::prelude::*;

    struct Foo;
    impl Service<u8> for Foo {
        type Response = u8;
        type Error = ();
        type Future = futures::future::FutureResult<Self::Response, Self::Error>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Ok(Async::Ready(()))
        }

        fn call(&mut self, req: u8) -> Self::Future {
            futures::future::ok(req)
        }
    }

    struct FooErr;
    impl Service<u8> for FooErr {
        type Response = ();
        type Error = u8;
        type Future = futures::future::FutureResult<Self::Response, Self::Error>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Ok(Async::Ready(()))
        }

        fn call(&mut self, req: u8) -> Self::Future {
            futures::future::err(req)
        }
    }

    macro_rules! assert_ready {
        ($e:expr) => {{
            match $e {
                Ok(futures::Async::Ready(v)) => v,
                Ok(_) => panic!("not ready"),
                Err(e) => panic!("error = {:?}", e),
            }
        }};
    }

    macro_rules! assert_err {
        ($e:expr) => {{
            match $e {
                Ok(futures::Async::Ready(v)) => panic!("succeeded = {:?}", v),
                Ok(_) => panic!("not ready"),
                Err(e) => e,
            }
        }};
    }

    #[test]
    fn test_map() {
        let mut mock = tokio_mock_task::MockTask::new();
        let mut svc = Map::new(Foo, |x| format!("Ok-{:?}", x));
        let mut fut = svc.call(12);
        let res = assert_err!(mock.enter(|| fut.poll()));
        assert_eq!(res, "Ok-12");
    }

    #[test]
    fn test_map_err() {
        let mut mock = tokio_mock_task::MockTask::new();
        let mut svc = MapErr::new(FooErr, |x| format!("Err-{:?}", x));
        let mut fut = svc.call(31);
        let res = assert_err!(mock.enter(|| fut.poll()));
        assert_eq!(res, "Err-31");
    }
}