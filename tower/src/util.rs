//! Combinators for working with `Service`s

pub use tower_util::BoxService;
pub use tower_util::CallAll;
pub use tower_util::CallAllUnordered;
pub use tower_util::Either;
pub use tower_util::Oneshot;
pub use tower_util::Optional;
pub use tower_util::Ready;
pub use tower_util::ServiceFn;
pub use tower_util::UnsyncBoxService;

use futures::Stream;
use tower_service::Service;

impl<T: ?Sized, Request> ServiceExt<Request> for T where T: Service<Request> {}

type Error = Box<dyn ::std::error::Error + Send + Sync>;

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
    fn call_all<S>(self, reqs: S) -> CallAll<Self, S>
    where
        Self: Sized,
        Self::Error: Into<Error>,
        S: Stream<Item = Request>,
        S::Error: Into<Error>,
    {
        CallAll::new(self, reqs)
    }
}
