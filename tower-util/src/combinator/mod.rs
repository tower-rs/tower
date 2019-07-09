//! Combinators for service responses.
//!
//! Apply transformations to the service responses and errors, much like
//! with `futures`.

use futures::{future, prelude::*};
use tower_layer::Layer;
use tower_service::Service;

mod and_then;
mod from_err;
mod inspect;
mod map;
mod map_err;
mod or_else;
mod then;
pub use and_then::*;
pub use from_err::*;
pub use inspect::*;
pub use map::*;
pub use map_err::*;
pub use or_else::*;
pub use then::*;

mod sealed {
    pub trait Sealed<T> {}
}

/// `Service` extension to provide combinator functionality equivalent to those
/// used in futures.
pub trait ServiceExt<Request>: sealed::Sealed<Request> + Service<Request> {
    /// Map this service's result to a different type, returning a new service.
    /// Equivalent to `futures::Future::map`.
    fn map<F, U>(self, f: F) -> Map<Self, F>
    where
        F: Fn(Self::Response) -> U,
        Self: Sized;

    /// Map this service's error to a different error, returning a new service.
    /// Equivalent to `futures::Future::map_err`.
    fn map_err<F, U>(self, f: F) -> MapErr<Self, F>
    where
        F: Fn(Self::Error) -> U,
        Self: Sized;

    /// Map this service's error to any error implementing `From` for this
    /// service's error, returning a new service.
    /// Equivalent to `futures::Future::from_err`.
    fn from_err<E>(self) -> FromErr<Self, E>
    where
        E: From<Self::Error>,
        Self: Sized;

    /// Chain on a computation for when a service returns from a call, passing
    /// the result to the provided closure `f`.
    /// Equivalent to `futures::Future::then`.
    ///
    /// TODO: Currently restricted to not modifying the error type,
    /// since we need `poll_ready` and `call` to have the same type.
    /// But it doesn't make sense to apply `then` to the `poll_ready` type.
    fn then<F, U>(self, f: F) -> Then<Self, F>
    where
        F: FnOnce(Result<Self::Response, Self::Error>) -> U,
        U: IntoFuture<Error = Self::Error>,
        Self: Sized;

    /// Execute another future after this service has resolved successfully.
    /// Equivalent to `futures::Future::and_then`.
    fn and_then<F, U>(self, f: F) -> AndThen<Self, F>
    where
        F: FnOnce(Self::Response) -> U,
        U: IntoFuture<Error = Self::Error>,
        Self: Sized;

    /// Execute another future if this service resolves with an error.
    /// Equivalent to `futures::Future::or_else`.
    ///
    /// TODO: Currently restricted to not modifying the error type,
    /// since we need `poll_ready` and `call` to have the same type.
    /// But it doesn't make sense to `or_else` the `poll_ready` type.
    fn or_else<F, U>(self, f: F) -> OrElse<Self, F>
    where
        F: FnOnce(Self::Error) -> U,
        U: IntoFuture<Item = Self::Response, Error = Self::Error>,
        Self: Sized;

    /// Do something with the result of a service, passing it on.
    /// Equivalent to `futures::Future::inspect`.
    fn inspect<F>(self, f: F) -> Inspect<Self, F>
    where
        F: FnOnce(&Self::Response),
        Self: Sized;
}

impl<T, Request> sealed::Sealed<Request> for T where T: Service<Request> {}

impl<T, Request> ServiceExt<Request> for T
where
    T: Service<Request>,
{
    fn map<F, U>(self, f: F) -> Map<Self, F>
    where
        F: Fn(Self::Response) -> U,
    {
        Map::new(self, f)
    }

    fn map_err<F, U>(self, f: F) -> MapErr<Self, F>
    where
        F: Fn(Self::Error) -> U,
    {
        MapErr::new(self, f)
    }

    fn from_err<E>(self) -> FromErr<Self, E>
    where
        E: From<Self::Error>,
    {
        FromErr::new(self)
    }

    fn then<F, U>(self, f: F) -> Then<Self, F>
    where
        F: FnOnce(Result<Self::Response, Self::Error>) -> U,
        U: IntoFuture<Error = Self::Error>,
    {
        Then::new(self, f)
    }

    fn and_then<F, U>(self, f: F) -> AndThen<Self, F>
    where
        F: FnOnce(Self::Response) -> U,
        U: IntoFuture<Error = Self::Error>,
    {
        AndThen::new(self, f)
    }

    fn or_else<F, U>(self, f: F) -> OrElse<Self, F>
    where
        F: FnOnce(Self::Error) -> U,
        U: IntoFuture<Item = Self::Response, Error = Self::Error>,
    {
        OrElse::new(self, f)
    }

    fn inspect<F>(self, f: F) -> Inspect<Self, F>
    where
        F: FnOnce(&Self::Response),
    {
        Inspect::new(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use futures::prelude::*;

    struct Foo;
    impl Service<u8> for Foo {
        type Response = u8;
        type Error = ();
        type Future = future::FutureResult<Self::Response, Self::Error>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Ok(Async::Ready(()))
        }

        fn call(&mut self, req: u8) -> Self::Future {
            future::ok(req)
        }
    }

    struct FooErr;
    impl Service<u8> for FooErr {
        type Response = ();
        type Error = u8;
        type Future = future::FutureResult<Self::Response, Self::Error>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Ok(Async::Ready(()))
        }

        fn call(&mut self, req: u8) -> Self::Future {
            future::err(req)
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
        let res = assert_ready!(mock.enter(|| fut.poll()));
        assert_eq!(res, "Ok-12");
    }

    #[test]
    fn test_map_layer() {
        let mut mock = tokio_mock_task::MockTask::new();
        let mut svc = tower::ServiceBuilder::new()
            .layer(MapLayer::new(|x| format!("Ok-{:?}", x)))
            .service(Foo);
        let mut fut = svc.call(12);
        let res = assert_ready!(mock.enter(|| fut.poll()));
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

    #[test]
    fn test_then() {
        let mut mock = tokio_mock_task::MockTask::new();
        let mut svc = Foo.then(|res| future::ok(format!("Ok-{:?}", res.unwrap())));
        let mut fut = svc.call(13);
        let res = assert_ready!(mock.enter(|| fut.poll()));
        assert_eq!(res, "Ok-13");
    }

    #[test]
    fn test_and_then() {
        let mut mock = tokio_mock_task::MockTask::new();
        let mut svc = Foo.then(|res| future::ok(format!("Ok-{:?}", res.unwrap())));
        let mut fut = svc.call(14);
        let res = assert_ready!(mock.enter(|| fut.poll()));
        assert_eq!(res, "Ok-14");
    }

    #[test]
    fn test_or_else() {
        let mut mock = tokio_mock_task::MockTask::new();
        let mut svc = FooErr
            .map(|_| "None".to_string())
            .or_else(|err| future::ok(format!("Ok-{:?}", err)));
        let mut fut = svc.call(15);
        let res = assert_ready!(mock.enter(|| fut.poll()));
        assert_eq!(res, "Ok-15");
    }

    fn view_val(_x: &u8) {}

    #[test]
    fn test_inspect() {
        let mut mock = tokio_mock_task::MockTask::new();
        let mut svc = Foo.inspect(view_val);
        let mut fut = svc.call(16);
        let res = assert_ready!(mock.enter(|| fut.poll()));
        assert_eq!(res, 16);
    }
}
