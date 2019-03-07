//! Combinators for working with `Service`s

use futures::{IntoFuture, Stream};
use tower_service::Service;

mod and_then;
mod apply;
mod call_all;
mod from_err;
mod map;
mod map_err;
mod oneshot;
mod ready;
mod then;

pub use self::and_then::AndThen;
pub use self::apply::Apply;
pub use self::call_all::{CallAll, StateStreamItem};
pub use self::from_err::FromErr;
pub use self::map::Map;
pub use self::map_err::MapErr;
pub use self::oneshot::Oneshot;
pub use self::ready::Ready;
pub use self::then::Then;

impl<T: ?Sized, Request> ServiceExt<Request> for T where T: Service<Request> {}

/// An extension trait for `Service`s that provides a variety of convenient
/// adapters
pub trait ServiceExt<Request>: Service<Request> {
    /// A future yielding the service when it is ready to accept a request.
    fn ready(self) -> Ready<Self, Request>
    where
        Self: Sized,
    {
        Ready::new(self)
    }

    fn apply<F, In, Out>(self, f: F) -> Apply<Self, F, In, Out, Request>
    where
        Self: Service<Request> + Clone + Sized,
        F: Fn(In, Self) -> Out,
        Out: IntoFuture<Error = Self::Error>,
    {
        Apply::new(f, self)
    }

    /// Call another service after call to this one has resolved successfully.
    ///
    /// This function can be used to chain two services together and ensure that
    /// the second service isn't called until call to the fist service have finished.
    /// Result of the call to the first service is used as an input parameter
    /// for the second service's call.
    ///
    /// Note that this function consumes the receiving service and returns a
    /// wrapped version of it.
    fn and_then<B>(self, service: B) -> AndThen<Self, B>
    where
        Self: Sized,
        B: Service<Self::Response, Error = Self::Error> + Clone,
    {
        AndThen::new(self, service)
    }

    /// Map this service's error to any error implementing `From` for
    /// this service`s `Error`.
    ///
    /// Note that this function consumes the receiving service and returns a
    /// wrapped version of it.
    fn from_err<E>(self) -> FromErr<Self, E>
    where
        Self: Sized,
        E: From<Self::Error>,
    {
        FromErr::new(self)
    }

    /// Chain on a computation for when a call to the service finished,
    /// passing the result of the call to the next service `B`.
    ///
    /// Note that this function consumes the receiving future and returns a
    /// wrapped version of it.
    fn then<B>(self, service: B) -> Then<Self, B>
    where
        Self: Sized,
        B: Service<Result<Self::Response, Self::Error>, Error = Self::Error> + Clone,
    {
        Then::new(self, service)
    }

    /// Map this service's output to a different type, returning a new service of
    /// the resulting type.
    ///
    /// This function is similar to the `Option::map` or `Iterator::map` where
    /// it will change the type of the underlying service.
    ///
    /// Note that this function consumes the receiving service and returns a
    /// wrapped version of it, similar to the existing `map` methods in the
    /// standard library.
    fn map<F, R>(self, f: F) -> Map<Self, F, R>
    where
        Self: Sized,
        F: Fn(Self::Response) -> R + Clone,
    {
        Map::new(self, f)
    }

    /// Map this service's error to a different error, returning a new service.
    ///
    /// This function is similar to the `Result::map_err` where it will change
    /// the error type of the underlying service. This is useful for example to
    /// ensure that services have the same error type.
    ///
    /// Note that this function consumes the receiving service and returns a
    /// wrapped version of it.
    fn map_err<F, E>(self, f: F) -> MapErr<Self, F, E>
    where
        Self: Sized,
        F: Fn(Self::Error) -> E + Clone,
    {
        MapErr::new(self, f)
    }

    /// Consume this `Service`, calling with the providing request once it is ready.
    fn oneshot(self, req: Request) -> Oneshot<Self, Request>
    where
        Self: Sized,
    {
        Oneshot::new(self, req)
    }

    /// Process all requests from the given `Stream`, and produce a `Stream` of their responses.
    ///
    /// This is essentially `Stream<Item = Request>` + `Self` => `Stream<Item = Response>`. See the
    /// documentation for [`CallAll`](struct.CallAll.html) for details.
    fn call_all<S, E>(self, reqs: S) -> CallAll<Self, S, E>
    where
        Self: Sized,
        S: Stream<Item = Request>,
    {
        CallAll::new(self, reqs)
    }
}
